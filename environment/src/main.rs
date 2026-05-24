mod cards;
use cards::CardType;
mod classic_battles;
use classic_battles::get_card_catalog;
mod card_effects;
mod decks;

fn main() {
    let c = get_card_catalog();
    let x = CardType::AttackAction;
    println!("Hello, world! {}", x as u8);
    println!("cards len {}", c.len());
}
