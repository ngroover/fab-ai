from typing import Dict

from card_effects import CardEffect, EffectTrigger, EffectAction
from cards import Card, CardType, Color, EquipSlot, CardClass, Keyword

CARD_CATALOG: Dict[str, Card] = {

    # ── HEROES ──────────────────────────────────────────────────────────────

    "rhinar": Card("Rhinar", CardType.HERO, card_class=CardClass.BRUTE,
                   text="Whenever you discard a card with 6 or more power during your action phase, intimidate.",
                   hero_life=20, hero_intellect=4, young=True,
                   effects=[
                       CardEffect(
                           trigger=EffectTrigger.ON_DISCARD,
                           action=EffectAction.INTIMIDATE,
                           condition=lambda ctx: ctx.get("card") is not None and ctx["card"].power >= 6,
                       )
                   ]),

    "dorinthea_quicksilver_prodigy": Card(
        "Dorinthea, Quicksilver Prodigy", CardType.HERO, card_class=CardClass.WARRIOR,
        text="The first time Dawnblade, Resplendent gains go again each turn, you may attack an additional time with it this turn.",
        hero_life=20, hero_intellect=4, young=True),

    # ── RHINAR — RED ────────────────────────────────────────────────────────

    "alpha_rampage_red": Card("Alpha Rampage", CardType.ACTION_ATTACK, cost=3, pitch=1,
                              power=9, defense=3, color=Color.RED,
                              card_class=CardClass.BRUTE,
                              keywords=[Keyword.INTIMIDATE],
                              text="Rhinar Specialization. As an additional cost to play Alpha Rampage, discard a random card. Intimidate.",
                              effects=[
                                  CardEffect(trigger=EffectTrigger.ON_ATTACK_PLAY, action=EffectAction.DISCARD_CARD_COST),
                              ]),

    "awakening_bellow_red": Card("Awakening Bellow", CardType.ACTION, cost=1, pitch=1,
                                 power=0, defense=3, color=Color.RED,
                                 keywords=[Keyword.GO_AGAIN, Keyword.INTIMIDATE],
                                 card_class=CardClass.BRUTE,
                                 text="Go again. Intimidate. Your next Brute attack action card this turn has +3 power.",
                                 effects=[
                                     CardEffect(trigger=EffectTrigger.ON_PLAY, action=EffectAction.NEXT_BRUTE_ATTACK_BONUS, magnitude=3),
                                 ]),

    "bare_fangs_red": Card("Bare Fangs", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           card_class=CardClass.BRUTE,
                           text="When you attack with Bare Fangs, draw a card then discard a random card. If a card with 6 or more power is discarded this way, Bare Fangs gets +2 power.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.DRAW_DISCARD_POWER_BONUS)]),

    "beast_mode_red": Card("Beast Mode", CardType.ACTION_ATTACK, cost=3, pitch=1,
                           power=6, defense=3, color=Color.RED,
                           card_class=CardClass.BRUTE,
                           text="If you have intimidated this turn, Beast Mode gains +2 power."),

    "pack_hunt_red": Card("Pack Hunt", CardType.ACTION_ATTACK, cost=2, pitch=1,
                          power=6, defense=3, color=Color.RED,
                          card_class=CardClass.BRUTE,
                          keywords=[Keyword.INTIMIDATE],
                          text="Intimidate."),

    "wild_ride_red": Card("Wild Ride", CardType.ACTION_ATTACK, cost=2, pitch=1,
                          power=6, defense=0, color=Color.RED, no_block=True,
                          card_class=CardClass.BRUTE,
                          text="When you attack with Wild Ride, draw a card then discard a random card. If a card with 6 or more power is discarded this way, Wild Ride gains go again.",
                          effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.DRAW_DISCARD_GO_AGAIN)]),

    "wrecking_ball_red": Card("Wrecking Ball", CardType.ACTION_ATTACK, cost=3, pitch=1,
                              power=6, defense=0, color=Color.RED, no_block=True,
                              card_class=CardClass.BRUTE,
                              text="When you attack with Wrecking Ball, draw a card then discard a random card. If a card with 6 or more power is discarded this way, intimidate.",
                              effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.DRAW_DISCARD_INTIMIDATE)]),

    # ── RHINAR — YELLOW ─────────────────────────────────────────────────────

    "barraging_beatdown_yellow": Card("Barraging Beatdown", CardType.ACTION, cost=0, pitch=2,
                                      power=0, defense=3, color=Color.YELLOW,
                                      keywords=[Keyword.GO_AGAIN, Keyword.INTIMIDATE],
                                      card_class=CardClass.BRUTE,
                                      text="Intimidate, then your next Brute attack this turn gains 'While this attack is defended by less than 2 non-equipment cards it has +3 power'. Go again."),

    "muscle_mutt_yellow": Card("Muscle Mutt", CardType.ACTION_ATTACK, cost=3, pitch=2,
                               power=6, defense=2, color=Color.YELLOW,
                               card_class=CardClass.BRUTE,
                               text=""),

    "pack_call_yellow": Card("Pack Call", CardType.ACTION_ATTACK, cost=3, pitch=2,
                             power=6, defense=3, color=Color.YELLOW,
                             card_class=CardClass.BRUTE,
                             text="When you defend with Pack Call, reveal the top card of your deck. If it has 6 or more power, put it on top of your deck. Otherwise, put it on the bottom.",
                             effects=[CardEffect(trigger=EffectTrigger.ON_DEFEND, action=EffectAction.REVEAL_TOP_DECK_POWER_CHECK)]),

    "raging_onslaught_yellow": Card("Raging Onslaught", CardType.ACTION_ATTACK, cost=3, pitch=2,
                                    power=6, defense=3, color=Color.YELLOW,
                                    card_class=CardClass.BRUTE,
                                    text=""),

    "smash_instinct_yellow": Card("Smash Instinct", CardType.ACTION_ATTACK, cost=3, pitch=2,
                                  power=6, defense=3, color=Color.YELLOW,
                                  card_class=CardClass.BRUTE,
                                  keywords=[Keyword.INTIMIDATE],
                                  text="Intimidate."),

    "smash_with_big_tree_yellow": Card("Smash with Big Tree", CardType.ACTION_ATTACK, cost=2, pitch=2,
                                       power=6, defense=0, color=Color.YELLOW, no_block=True,
                                       card_class=CardClass.BRUTE,
                                       text=""),

    "wounded_bull_yellow": Card("Wounded Bull", CardType.ACTION_ATTACK, cost=3, pitch=2,
                                power=6, defense=2, color=Color.YELLOW,
                                card_class=CardClass.BRUTE,
                                text="When you play Wounded Bull, if you have less health than an opposing hero, it gains +1 power."),

    # ── RHINAR — BLUE ───────────────────────────────────────────────────────

    "clearing_bellow_blue": Card("Clearing Bellow", CardType.ACTION, cost=0, pitch=3,
                                 power=0, defense=3, color=Color.BLUE,
                                 keywords=[Keyword.GO_AGAIN, Keyword.INTIMIDATE],
                                 card_class=CardClass.BRUTE,
                                 text="Intimidate. Go again."),

    "come_to_fight_blue": Card("Come to Fight", CardType.ACTION, cost=1, pitch=3,
                               power=0, defense=3, color=Color.BLUE,
                               text="Your next attack action card you play this turn gains +1 power. Go again."),

    "dodge_blue": Card("Dodge", CardType.DEFENSE_REACTION, cost=0, pitch=3,
                       power=0, defense=2, color=Color.BLUE,
                       no_block=True,
                       text=""),

    "rally_the_rearguard_blue": Card("Rally the Rearguard", CardType.ACTION_ATTACK, cost=2, pitch=3,
                                     power=4, defense=2, color=Color.BLUE,
                                     text="Once per turn Instant - Discard a card: Rally the Rearguard gains +3 block.  Activate this ability only while Rally the Rearguard is defending."),

    "titanium_bauble_blue": Card("Titanium Bauble", CardType.RESOURCE, cost=0, pitch=3,
                                 power=0, defense=3, color=Color.BLUE,
                                 text=""),

    "wrecker_romp_blue": Card("Wrecker Romp", CardType.ACTION_ATTACK, cost=2, pitch=3,
                              power=6, defense=3, color=Color.BLUE,
                              card_class=CardClass.BRUTE,
                              text="As an additional cost to play Wrecker Romp, discard a card.",
                              effects=[CardEffect(trigger=EffectTrigger.ON_PLAY, action=EffectAction.DISCARD_CARD_COST)]),

    # ── RHINAR — MENTOR ─────────────────────────────────────────────────────

    "chief_ruk_utan": Card("Chief Ruk'utan", CardType.MENTOR, cost=0, pitch=0,
                           power=0, defense=0,
                           card_class=CardClass.BRUTE,
                           text="While Ruk'utan is face down in arsenal, at the start of your turn, you may turn him face up.  While Ruk'utan is face up in arsenal, whenever you play a card with 6 or more power, intimidate and put a lesson counter on him.  Then if there are 2 or more lesson counters on Rok'utan, banish him, search your deck for Alpha Rampage, put it face up in arsenal and shuffle."),

    # ── RHINAR — EQUIPMENT ──────────────────────────────────────────────────

    "bone_basher": Card("Bone Basher", CardType.WEAPON, cost=2, power=4, equip_slot=EquipSlot.WEAPON,
                        card_class=CardClass.BRUTE,
                        text="Once per Turn Action — 2: Attack."),

    "blossom_of_spring": Card("Blossom of Spring", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.CHEST,
                              text="Action: Destroy Blossom of Spring: Gain 1 resource. Go again"),

    "bone_vizier": Card("Bone Vizier", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.HEAD,
                        card_class=CardClass.BRUTE,
                        text="When Bone Vizier is destroyed, reveal the top card of your deck.  If it has 6 or more power, put it on the top of your deck. Otherwise, put it on the bottom. Blade Break",
                        keywords=[Keyword.BLADE_BREAK],
                        effects=[CardEffect(trigger=EffectTrigger.ON_DESTROYED, action=EffectAction.REVEAL_TOP_DECK_POWER_CHECK)]),

    "ironhide_gauntlet": Card("Ironhide Gauntlet", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.ARMS,
                              card_class=CardClass.BRUTE,
                              text="When you defend with Ironhide Gauntlet you may pay 1 resource.  If you do, it gains +2 block and 'When the combat chain closes, destroy Ironhide Gauntlets'"),

    "ironhide_legs": Card("Ironhide Legs", CardType.EQUIPMENT, defense=0, equip_slot=EquipSlot.LEGS,
                          card_class=CardClass.BRUTE,
                          text="When you defend with Ironhide Legs you may pay 1 resource.  If you do, it gains +2 block and 'When the combat chain closes, destroy Ironhide Legs'"),

    # ── DORINTHEA — RED ─────────────────────────────────────────────────────

    "en_garde_red": Card("En Garde", CardType.ACTION, cost=1, pitch=1,
                         power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                         card_class=CardClass.WARRIOR,
                         text="Your next weapon attack this turn gains +3 power. Go again."),

    "flock_of_the_feather_walkers_red": Card(
        "Flock of the Feather Walkers", CardType.ACTION_ATTACK, cost=1, pitch=1,
        power=5, defense=2, color=Color.RED,
        card_class=CardClass.GENERIC,
        text="As an additional cost to play Flock of the Feather Walkers, reveal a card in your hand with cost 1 or less.  When you attack with Flock of the Feather Walkers, create a Quicken token.",
        effects=[
            CardEffect(trigger=EffectTrigger.ON_ATTACK_PLAY, action=EffectAction.REVEAL_CARD_COST),
            CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.QUICKEN_TOKEN),
        ]),

    "in_the_swing_red": Card("In the Swing", CardType.ATTACK_REACTION, cost=0, pitch=1,
                             power=0, defense=3, color=Color.RED,
                             card_class=CardClass.WARRIOR,
                             text="Play only if you have attacked 2 or more times with weapons this turn. Target weapon attack gains +3 power.",
                             play_condition=lambda ctx: ctx.get("weapon_attack_count", 0) >= 1,
                             effects=[CardEffect(
                                 trigger=EffectTrigger.ON_ATTACK_REACTION,
                                 action=EffectAction.ATTACK_POWER_BOOST,
                                 magnitude=3,
                                 condition=lambda ctx: ctx.get("weapon_attack_count", 0) >= 1,
                             )]),

    "ironsong_response_red": Card("Ironsong Response", CardType.ATTACK_REACTION, cost=0, pitch=1,
                                  power=0, defense=3, color=Color.RED,
                                  card_class=CardClass.WARRIOR,
                                  text="Reprise - If the defending hero has defended with a card from their hand this chain link, your weapon attack gains +3 power."),

    "second_swing_red": Card("Second Swing", CardType.ACTION, cost=1, pitch=1,
                             power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                             card_class=CardClass.WARRIOR,
                             text="If you have attacked with a weapon this turn, your next attack this turn gains +4 power. Go again.",
                             effects=[
                                 CardEffect(
                                     trigger=EffectTrigger.ON_PLAY,
                                     action=EffectAction.WEAPON_ATTACK_POWER_BONUS,
                                     magnitude=4,
                                     condition=lambda ctx: ctx.get("weapon_attack_count", 0) >= 1,
                                 )
                             ]),

    "sharpen_steel_red": Card("Sharpen Steel", CardType.ACTION, cost=0, pitch=1,
                              power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                              card_class=CardClass.WARRIOR,
                              text="Your next weapon attack this turn gains +3 power. Go again.",
                              effects=[CardEffect(
                                  trigger=EffectTrigger.ON_PLAY,
                                  action=EffectAction.WEAPON_ATTACK_POWER_BONUS,
                                  magnitude=3,
                              )]),

    "thrust_red": Card("Thrust", CardType.ATTACK_REACTION, cost=1, pitch=1,
                       power=0, defense=2, color=Color.RED,
                       card_class=CardClass.WARRIOR,
                       text="Target sword attack gains +3 power.",
                       effects=[CardEffect(
                           trigger=EffectTrigger.ON_ATTACK_REACTION,
                           action=EffectAction.ATTACK_POWER_BOOST,
                           magnitude=3,
                       )]),

    "warrior_s_valor_red": Card("Warrior's Valor", CardType.ACTION, cost=1, pitch=1,
                                power=0, defense=3, color=Color.RED, keywords=[Keyword.GO_AGAIN],
                                card_class=CardClass.WARRIOR,
                                text="Your next weapon attack this turn gains +3 power and 'When this attack hits, it gains go again.' Go again."),

    # ── DORINTHEA — YELLOW ──────────────────────────────────────────────────

    "driving_blade_yellow": Card("Driving Blade", CardType.ACTION, cost=2, pitch=2,
                                 power=0, defense=3, color=Color.YELLOW,
                                 keywords=[Keyword.GO_AGAIN],
                                 card_class=CardClass.WARRIOR,
                                 text="Your next weapon attack this turn gains +2 power and go again. Go again.",
                                 effects=[
                                     CardEffect(trigger=EffectTrigger.ON_PLAY, action=EffectAction.NEXT_WEAPON_POWER_BONUS, magnitude=2),
                                     CardEffect(trigger=EffectTrigger.ON_PLAY, action=EffectAction.NEXT_WEAPON_GO_AGAIN),
                                 ]),

    "glistening_steelblade_yellow": Card("Glistening Steelblade", CardType.ACTION, cost=1, pitch=2,
                                         power=0, defense=3, color=Color.YELLOW, keywords=[Keyword.GO_AGAIN],
                                         card_class=CardClass.WARRIOR,
                                         text="Dorinthea Specialization. Your next Dawnblade attack this turn has go again.  Whenever Dawnblade hits a hero this turn, put a +1 counter on it. Go again."),

    "on_a_knife_edge_yellow": Card("On a Knife Edge", CardType.ACTION, cost=0, pitch=2,
                                   power=0, defense=2, color=Color.YELLOW, keywords=[Keyword.GO_AGAIN],
                                   card_class=CardClass.WARRIOR,
                                   text="Your next sword attack this turn gains go again. Go again."),

    "out_for_blood_yellow": Card("Out for Blood", CardType.ATTACK_REACTION, cost=1, pitch=2,
                                 power=0, defense=3, color=Color.YELLOW,
                                 card_class=CardClass.WARRIOR,
                                 text="Target weaopn attack gains +2 power.  Reprise - If the defending hero has defended with a card from their hand this chain link, your next attack this turn gains +1 power."),

    "run_through_yellow": Card("Run Through", CardType.ATTACK_REACTION, cost=1, pitch=2,
                               power=0, defense=3, color=Color.YELLOW,
                               card_class=CardClass.WARRIOR,
                               text="Target sword attack gains go again.  Your next sword attack this turn gains +2 power.",
                               effects=[
                                   CardEffect(trigger=EffectTrigger.ON_ATTACK_REACTION, action=EffectAction.SWORD_ATTACK_GO_AGAIN),
                                   CardEffect(trigger=EffectTrigger.ON_ATTACK_REACTION, action=EffectAction.NEXT_SWORD_ATTACK_POWER_BONUS, magnitude=2),
                               ]),

    "slice_and_dice_yellow": Card("Slice and Dice", CardType.ACTION, cost=0, pitch=2,
                                  power=0, defense=3, color=Color.YELLOW, keywords=[Keyword.GO_AGAIN],
                                  card_class=CardClass.WARRIOR,
                                  text="The first time you attack with a weapon this turn, if it's a sword or dagger it gains +1 power.  The second time you attack with a weapon this turn if it's a sword or dagger it gains +2 power.  Go again.",
                                  effects=[CardEffect(trigger=EffectTrigger.ON_PLAY,
                                                      action=EffectAction.WEAPON_ATTACK_BONUS_PER_SWING)]),

    # ── DORINTHEA — BLUE ────────────────────────────────────────────────────

    "blade_flash_blue": Card("Blade Flash", CardType.ATTACK_REACTION, cost=1, pitch=3,
                             power=0, defense=2, color=Color.BLUE,
                             card_class=CardClass.WARRIOR,
                             text="Target sword attack gains go again.",
                             effects=[CardEffect(EffectTrigger.ON_ATTACK_REACTION,
                                                 EffectAction.SWORD_ATTACK_GO_AGAIN)]),

    "hit_and_run_blue": Card("Hit and Run", CardType.ACTION, cost=0, pitch=3,
                             power=0, defense=3, color=Color.BLUE, keywords=[Keyword.GO_AGAIN],
                             card_class=CardClass.WARRIOR,
                             text="Your next weapon attack this turn gains go again.  If you have attacked with a weapon this turn your next attack this turn gains +1 power.  Go again."),

    "sigil_of_solace_blue": Card("Sigil of Solace", CardType.INSTANT, cost=0, pitch=3,
                                 power=0, defense=0, color=Color.BLUE, no_block=True,
                                 text="Gain 1 life."),

    "toughen_up_blue": Card("Toughen Up", CardType.DEFENSE_REACTION, cost=2, pitch=3,
                            power=0, defense=4, color=Color.BLUE,
                            no_block=True, # this card cannot block normally it can block as defense reaction in the defense reaction step
                            text=""),

    "visit_the_blacksmith_blue": Card("Visit the Blacksmith", CardType.ACTION, cost=0, pitch=3,
                                      power=0, defense=2, color=Color.BLUE, keywords=[Keyword.GO_AGAIN],
                                      card_class=CardClass.WARRIOR,
                                      text="Your next sword attack this turn gains +1 power."),

    # ── DORINTHEA — MENTOR ──────────────────────────────────────────────────

    "hala_goldenhelm": Card("Hala Goldenhelm", CardType.MENTOR, cost=0, pitch=0,
                            power=0, defense=3,
                            card_class=CardClass.WARRIOR,
                            text="While Hala is face down in arsenal, at the start of your turn you may turn her face up.  While Hala is face up in arsenal, whenever a sword attack you control hits, it gains go again and put a lesson counter on Hala.  Then if there are 2 or more lesson counters on Hala banish her, search your deck for Glistening Steelblade, put it face up in arsenal, and shuffle."),

    # ── DORINTHEA — EQUIPMENT ───────────────────────────────────────────────

    "dawnblade_resplendent": Card("Dawnblade, Resplendent", CardType.WEAPON, cost=1, power=2,
                                  equip_slot=EquipSlot.WEAPON,
                                  card_class=CardClass.WARRIOR,
                                  text="Once per Turn Action — 1: Attack. The second time you attack with Dawnblade each turn, it gains +1 power until the end of turn."),

    "gallantry_gold": Card("Gallantry Gold", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.ARMS,
                           card_class=CardClass.WARRIOR,
                           text="Action — 1: destroy Gallantry Gold: Your weapon attacks gain +1 power this turn. Go again. Battleworn."),

    "ironrot_helm": Card("Ironrot Helm", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.ARMS,
                         text="Blade Break.", keywords=[Keyword.BLADE_BREAK]),

    "ironrot_legs": Card("Ironrot Legs", CardType.EQUIPMENT, defense=1, equip_slot=EquipSlot.LEGS,
                         text="Blade Break.", keywords=[Keyword.BLADE_BREAK]),
}
