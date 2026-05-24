use crate::cards::Card;

#[derive(Default)]
pub struct CardArea {
    cards : [Option<Card>; 40],
    first : u8,
    last: u8,
}

impl Default for CardArea {
    fn default() -> Self {
        Self {
            cards: [None; 40],
            first: 0,
            last: 0,
        }
    }
}

pub struct Player {
    pub life: u8,
    pub intellect: u8,
    pub hero : Card,
    pub deck : CardArea,
//
    //deck : Card
}
