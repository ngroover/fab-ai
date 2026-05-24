mod crate::cards;
use cards::CardData;

pub fn get_card_catalog() -> [CardData; 1] {
    let cat : [CardData; 1] = [
        // Rhinar
        CardData{typ: CardType::Hero}
    ];
    cat
}

