//! Human-readable text for the terminal: card names, action descriptions and
//! a printout of the game state. Read-only over the engine; used by the
//! interactive play mode (`cli`, `human_agent`).

use std::fmt::Write;

use crate::action::{Action, ActionType};
use crate::card_counts::CardCounts;
use crate::cards::{Card, CardType, Color};
use crate::deck_cards::TokenCard;
use crate::game_state::{Gamestate, PendingCard, Phase, Player, PlayerIndex, Zone, CHAIN_LINKS};

/// "P1" / "P2", matching the names the game log uses.
pub fn player_name(pid: PlayerIndex) -> &'static str {
    if pid == PlayerIndex::P1 { "P1" } else { "P2" }
}

/// A card's name as printed on it, rebuilt from its `Card` variant:
/// `WreckingBallR` becomes "Wrecking Ball (red)".
pub fn card_name(card: Card) -> String {
    let ident = format!("{:?}", card);
    let (ident, color) = match card.data().color {
        Some(Color::Red) => (&ident[..ident.len() - 1], " (red)"),
        Some(Color::Yellow) => (&ident[..ident.len() - 1], " (yellow)"),
        Some(Color::Blue) => (&ident[..ident.len() - 1], " (blue)"),
        None => (&ident[..], ""),
    };
    let mut name = String::new();
    for (i, ch) in ident.chars().enumerate() {
        if i > 0 && ch.is_uppercase() {
            name.push(' ');
        }
        name.push(ch);
    }
    name + color
}

fn type_name(typ: CardType) -> &'static str {
    match typ {
        CardType::AttackAction => "attack action",
        CardType::Action => "action",
        CardType::Instant => "instant",
        CardType::AttackReaction => "attack reaction",
        CardType::DefenseReaction => "defense reaction",
        CardType::Equipment => "equipment",
        CardType::Weapon => "weapon",
        CardType::Hero => "hero",
        CardType::Mentor => "mentor",
        CardType::Resource => "resource",
        CardType::Token => "token",
    }
}

/// A card's type and the numbers that matter for it, e.g.
/// "attack action, cost 2, pitch 1, power 6, defense 3". `owner` supplies the
/// defense counters worn equipment has picked up.
pub fn card_stats(owner: &Player, card: Card) -> String {
    let data = card.data();
    let mut parts = vec![type_name(data.typ).to_string()];
    let played = matches!(
        data.typ,
        CardType::AttackAction | CardType::Action | CardType::Instant
            | CardType::AttackReaction | CardType::DefenseReaction | CardType::Resource
    );
    if played {
        parts.push(format!("cost {}", data.cost));
    }
    if let Some(ability) = &data.ability {
        parts.push(format!("ability cost {}", ability.resource_cost()));
    }
    if data.pitch > 0 {
        parts.push(format!("pitch {}", data.pitch));
    }
    if data.power > 0 {
        parts.push(format!("power {}", data.power));
    }
    let blocks = (played || data.typ == CardType::Equipment) && !data.no_block;
    if blocks {
        parts.push(format!("defense {}", owner.defense_of(data)));
    }
    parts.join(", ")
}

fn card_with_stats(owner: &Player, card: Card) -> String {
    format!("{} [{}]", card_name(card), card_stats(owner, card))
}

fn zone_name(zone: Zone) -> &'static str {
    match zone {
        Zone::Hand => "hand",
        Zone::Deck => "deck",
        Zone::Pitch => "pitch zone",
        Zone::Graveyard => "graveyard",
        Zone::Arsenal => "arsenal",
        Zone::Banish => "banish zone",
        Zone::IntimidateBanish => "intimidate banish zone",
        Zone::Equipment => "equipment",
        Zone::Weapon => "weapon zone",
        Zone::CombatChain => "combat chain",
        Zone::Stack => "stack",
    }
}

pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Start => "start",
        Phase::ChooseFirst => "choose who goes first",
        Phase::MentorFlip => "start of turn (mentor)",
        Phase::Action => "action",
        Phase::ActionPitch => "pitch (action)",
        Phase::ActionInstant => "priority (instants)",
        Phase::Defend => "declare blockers",
        Phase::DefendTriggers => "defend triggers",
        Phase::DefendPitch => "pitch (defend)",
        Phase::DefendPayment => "defend trigger payment",
        Phase::RevealChoice => "choose a card to reveal",
        Phase::Reaction => "reactions",
        Phase::ReactionPitch => "pitch (reaction)",
        Phase::Arsenal => "arsenal",
        Phase::PitchOrder => "order pitched cards",
        Phase::Player1Win => "game over: P1 wins",
        Phase::Player2Win => "game over: P2 wins",
        Phase::Draw => "game over: draw",
    }
}

/// What `action` means for the player about to take it, as a menu line.
pub fn describe_action(gs: &Gamestate, action: &Action) -> String {
    let player = gs.player(gs.active_player);
    let Some(card_ref) = action.card else {
        return match (action.typ, gs.phase) {
            (ActionType::ChooseFirst, _) => "Go first".into(),
            (ActionType::ChooseSecond, _) => "Go second".into(),
            (ActionType::CloseChain, _) => "Break the combat chain".into(),
            (ActionType::Pass, Phase::Defend) => "Pass (done declaring blockers)".into(),
            (ActionType::Pass, Phase::Arsenal) => "Pass (arsenal nothing)".into(),
            (ActionType::Pass, Phase::DefendPayment) => "Pass (decline to pay)".into(),
            (ActionType::Pass, Phase::MentorFlip) => "Pass (leave the mentor face down)".into(),
            (ActionType::Pass, _) => "Pass".into(),
            (typ, _) => format!("{:?}", typ),
        };
    };
    let card = player.card(card_ref.id);
    let full = card_with_stats(player, card);
    let zone = zone_name(card_ref.zone);
    match action.typ {
        ActionType::PlayCard => format!("Play {} from {}", full, zone),
        ActionType::Activate if gs.phase == Phase::DefendPayment => {
            format!("Pay for {}'s defend trigger", card_name(card))
        }
        ActionType::Activate if card.data().typ == CardType::Weapon => format!("Attack with {}", full),
        ActionType::Activate => format!("Activate {} ({})", full, zone),
        ActionType::Pitch => format!("Pitch {} for {}", card_name(card), card.data().pitch),
        ActionType::BottomPitch => format!("Put {} on the bottom of your deck", card_name(card)),
        ActionType::Defend => format!("Defend with {} from {}", full, zone),
        ActionType::Arsenal => format!("Put {} into your arsenal", full),
        ActionType::FlipFaceUp => format!("Turn {} face up", card_name(card)),
        ActionType::Reveal => format!("Reveal {}", card_name(card)),
        typ => format!("{:?} {}", typ, full),
    }
}

/// Every copy in `counts`, by name, one entry per distinct card.
fn list_cards(owner: &Player, counts: CardCounts, stats: bool) -> String {
    if counts.is_empty() {
        return "-".into();
    }
    counts
        .iter()
        .map(|(id, n)| {
            let card = owner.card(id);
            let name = if stats { card_with_stats(owner, card) } else { card_name(card) };
            if n > 1 { format!("{}x {}", n, name) } else { name }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn describe_pending(gs: &Gamestate, p: &PendingCard) -> String {
    let owner = gs.player(p.owner);
    let what = match p.typ {
        ActionType::PlayCard => "plays",
        ActionType::Activate => "activates",
        ActionType::DefendTrigger => "defend trigger of",
        _ => "uses",
    };
    format!("{} {} {}", player_name(p.owner), what, card_with_stats(owner, owner.card(p.card)))
}

/// The game state as text. With `viewer` set, that player's opponent's hidden
/// cards (hand, face-down arsenal, intimidated cards) are shown only as far as
/// the viewer legitimately knows them; with `None` everything is shown.
pub fn render_state(gs: &Gamestate, viewer: Option<PlayerIndex>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "================ GAME STATE ================");
    let _ = writeln!(
        out,
        "Turn {} | {}'s turn | phase: {} | {} to act",
        gs.turn_count,
        player_name(gs.turn_player),
        phase_name(gs.phase),
        player_name(gs.active_player),
    );

    if let Some(p) = &gs.pending_card {
        let _ = writeln!(out, "Being paid for: {}", describe_pending(gs, p));
    }
    if gs.stack.is_empty() {
        let _ = writeln!(out, "Stack: empty");
    } else {
        let _ = writeln!(out, "Stack (top resolves first):");
        for (i, p) in gs.stack.iter().rev().enumerate() {
            let _ = writeln!(out, "  {}. {}", i + 1, describe_pending(gs, &p));
        }
    }

    let attacker = gs.player(gs.turn_player);
    let defender = gs.player(gs.turn_player.opponent());
    if attacker.chain_occupied() || defender.chain_occupied() {
        let _ = writeln!(out, "Combat chain:");
        for link in 0..CHAIN_LINKS {
            let (atk, def) = (attacker.chain_link[link], defender.chain_link[link]);
            if atk.is_empty() && def.is_empty() {
                continue;
            }
            let bonus = defender.chain_defense_bonus[link];
            let bonus = if bonus > 0 { format!(" (+{} defense)", bonus) } else { String::new() };
            let _ = writeln!(
                out,
                "  Link {}: {} attacks with {} | {} blocks with {}{}",
                link + 1,
                player_name(attacker.pid),
                list_cards(attacker, atk, true),
                player_name(defender.pid),
                list_cards(defender, def, true),
                bonus,
            );
        }
    } else {
        let _ = writeln!(out, "Combat chain: empty");
    }

    for pid in [PlayerIndex::P1, PlayerIndex::P2] {
        render_player(&mut out, gs.player(pid), viewer);
    }
    let _ = write!(out, "============================================");
    out
}

fn render_player(out: &mut String, p: &Player, viewer: Option<PlayerIndex>) {
    // Whether the viewer may see this player's hidden cards.
    let sees_hidden = viewer.is_none_or(|v| v == p.pid);
    let you = if viewer == Some(p.pid) { " (you)" } else { "" };
    let _ = writeln!(out, "--------------------------------------------");
    let _ = writeln!(
        out,
        "{}{} - {} | life {} | resources {} | action points {}",
        player_name(p.pid), you, card_name(p.hero), p.life, p.resources, p.action_points,
    );

    if sees_hidden {
        let _ = writeln!(out, "  Hand ({}): {}", p.hand_size(), list_cards(p, p.hand, true));
    } else if p.hand_known.is_empty() {
        let _ = writeln!(out, "  Hand: {} hidden card(s)", p.hand_size());
    } else {
        let _ = writeln!(
            out,
            "  Hand: {} card(s), known to you: {}",
            p.hand_size(),
            list_cards(p, p.hand_known, true),
        );
    }

    let arsenal = match p.arsenal {
        None => "-".to_string(),
        Some(id) => {
            let facing = if p.arsenal_face_up() { "face up" } else { "face down" };
            if sees_hidden || p.arsenal_face_up() || p.arsenal_known() {
                let counters = match p.arsenal_lesson_counters {
                    0 => String::new(),
                    n => format!(", {} lesson counter(s)", n),
                };
                format!("{} ({}{})", card_with_stats(p, p.card(id)), facing, counters)
            } else {
                "a face-down card".to_string()
            }
        }
    };
    let _ = writeln!(out, "  Arsenal: {}", arsenal);

    let weapon = match p.weapon {
        Some(id) => {
            let counters = match p.weapon_power_counters {
                0 => String::new(),
                n => format!(" with {} +1 power counter(s)", n),
            };
            format!("{}{}", card_with_stats(p, p.card(id)), counters)
        }
        None => "- (attacking, or none)".to_string(),
    };
    let _ = writeln!(out, "  Weapon: {}", weapon);
    let _ = writeln!(out, "  Equipment: {}", list_cards(p, p.equipment, true));
    let quickens = p.token_count(TokenCard::QuickenToken);
    if quickens > 0 {
        let _ = writeln!(out, "  Tokens: {}x Quicken", quickens);
    }
    let _ = writeln!(out, "  Pitch zone: {}", list_cards(p, p.pitch, false));
    let _ = writeln!(out, "  Graveyard ({}): {}", p.graveyard.len(), list_cards(p, p.graveyard, false));
    let _ = writeln!(out, "  Banished ({}): {}", p.banished.len(), list_cards(p, p.banished, false));
    if !p.intimidate_banished.is_empty() {
        let cards = if sees_hidden {
            list_cards(p, p.intimidate_banished, false)
        } else {
            format!("{} hidden card(s)", p.intimidate_banished.len())
        };
        let _ = writeln!(out, "  Intimidated (returns at end of turn): {}", cards);
    }
    let _ = writeln!(out, "  Deck: {} card(s)", p.deck_size());

    let effects = active_effects(p);
    if !effects.is_empty() {
        let _ = writeln!(out, "  Effects this turn: {}", effects.join("; "));
    }
}

/// The bonuses banked on `p` for this turn, as short phrases.
fn active_effects(p: &Player) -> Vec<String> {
    let mut effects = Vec::new();
    let mut bonus = |n: u8, what: &str| {
        if n > 0 {
            effects.push(format!("+{} power {}", n, what));
        }
    };
    bonus(p.attack_power_bonus, "on the current attack");
    bonus(p.next_attack_bonus, "on the next attack");
    bonus(p.next_attack_action_bonus, "on the next attack action card");
    bonus(p.next_brute_attack_action_bonus, "on the next Brute attack action card");
    bonus(p.next_brute_attack_conditional_bonus, "on the next Brute attack if defended by fewer than 2 non-equipment cards");
    bonus(p.next_weapon_attack_bonus, "on the next weapon attack");
    bonus(p.next_sword_attack_bonus, "on the next sword attack");
    bonus(p.weapon_attack_power[0], "on the first weapon attack");
    bonus(p.weapon_attack_power[1], "on the second weapon attack");
    bonus(p.weapon_attacks_power_this_turn, "on every weapon attack");
    if p.dawnblade_hit_counter_effects > 0 {
        effects.push(format!("Dawnblade gains {} counter(s) per hit", p.dawnblade_hit_counter_effects));
    }
    let mut flag = |on: bool, what: &str| {
        if on {
            effects.push(what.to_string());
        }
    };
    flag(p.attack_go_again_bonus(), "current attack has go again");
    flag(p.next_weapon_go_again(), "next weapon attack gains go again");
    flag(p.next_weapon_on_hit_go_again(), "next weapon attack gains go again if it hits");
    flag(p.next_dawnblade_go_again(), "next Dawnblade attack gains go again");
    flag(p.next_sword_go_again(), "next sword attack gains go again");
    flag(p.extra_weapon_attack(), "may attack with the weapon an additional time");
    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::CardRef;
    use crate::decks::{build_dorinthea_deck, build_rhinar_deck};
    use crate::deck_cards::RhinarCard;
    use crate::test_support::new_game;

    #[test]
    fn card_names_are_spaced_and_coloured() {
        assert_eq!(card_name(Card::WreckingBallR), "Wrecking Ball (red)");
        assert_eq!(card_name(Card::OnAKnifeEdgeY), "On A Knife Edge (yellow)");
        assert_eq!(card_name(Card::TitaniumBaubleB), "Titanium Bauble (blue)");
        assert_eq!(card_name(Card::BoneBasher), "Bone Basher");
        assert_eq!(card_name(Card::Rhinar), "Rhinar");
    }

    #[test]
    fn every_card_has_a_name_and_stats() {
        let gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        for (player, deck) in [(&gs.p1, build_rhinar_deck()), (&gs.p2, build_dorinthea_deck())] {
            for card in deck {
                assert!(!card_name(card).is_empty());
                assert!(!card_stats(player, card).is_empty());
            }
        }
    }

    #[test]
    fn actions_are_described_by_card_and_zone() {
        let gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        let play = Action::new(ActionType::PlayCard, CardRef::new(RhinarCard::WreckingBallR.id(), Zone::Hand));
        let mut gs_p1 = gs.clone();
        gs_p1.active_player = PlayerIndex::P1;
        let text = describe_action(&gs_p1, &play);
        assert!(text.starts_with("Play Wrecking Ball (red) ["), "{}", text);
        assert!(text.ends_with("from hand"), "{}", text);
        assert_eq!(describe_action(&gs, &Action::no_card(ActionType::ChooseFirst)), "Go first");
    }

    #[test]
    fn the_opponents_hand_is_hidden_from_the_viewer() {
        let mut gs = new_game(build_rhinar_deck(), build_dorinthea_deck(), Some(42), false);
        gs.p1.hand.add(RhinarCard::WreckingBallR.id());
        gs.p1.deck.pop_front();
        let name = card_name(Card::WreckingBallR);
        assert!(render_state(&gs, Some(PlayerIndex::P1)).contains(&name));
        assert!(render_state(&gs, None).contains(&name));
        assert!(!render_state(&gs, Some(PlayerIndex::P2)).contains(&name));
    }
}
