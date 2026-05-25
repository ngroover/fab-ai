mod cards;
mod classic_battles;
mod card_effects;
mod decks;
mod game_state;
use cards::CardType;
use classic_battles::get_card_catalog;
use cards::Card;
use game_state::{Player,CardArea};

fn main() {
    let c = get_card_catalog();
    let x = CardType::AttackAction;
    let r = Card::Rhinar;
    let rhin = &c[r as usize];
    let p = Player{ life: 20,
                intellect: 4,
                hero: Card::Rhinar,
                deck: CardArea::default()};

    println!("Hello, world! {}", x as u8);
    println!("cards len {}", c.len());
    println!("rhinar hp is {}", rhin.hero_life);
    println!("player life is {}", p.life);
}
