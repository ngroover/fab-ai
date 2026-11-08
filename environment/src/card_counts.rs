use crate::deck_cards::DeckCard;

/// Bits spent on each deck card id. Two bits count 0..=3 copies; a Blitz deck
/// holds at most two of any card, so the third value is headroom rather than
/// need.
const BITS_PER_CARD: usize = 2;
/// The largest count a single deck card id can hold.
const MAX_COPIES: u8 = (1 << BITS_PER_CARD) - 1;
/// The low bit of every card's field (`0b0101…`). Masking with it and with
/// `>> 1` splits the packed counts into their ones and twos bits.
const LOW_BITS: u64 = u64::MAX / 0b11;

/// How many distinct cards one player's deck may contain: each needs its own
/// `BITS_PER_CARD`-bit field in the `u64` behind `CardCounts`. `deck_cards!`
/// checks every deck enum against this at compile time.
pub const MAX_DECK_IDS: usize = u64::BITS as usize / BITS_PER_CARD;

/// A card's id *within its owner's deck*, in `0..MAX_DECK_IDS`.
///
/// `Card` numbers every card in the game, which is far more than any one deck
/// holds. Each deck instead has its own compile-time enum (`RhinarCard`,
/// `DorintheaCard`; see `deck_cards`) whose discriminant is this id, and the id
/// is the bit position of that card's count in a `CardCounts`.
///
/// The same `Card` can have different `DeckCardId`s in different decks, so an
/// id means nothing without the `DeckKind` it came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct DeckCardId(u8);

impl DeckCardId {
    /// Wrap a raw id. Panics if it does not fit in a `CardCounts`.
    #[inline]
    pub const fn new(i: usize) -> Self {
        assert!(i < MAX_DECK_IDS, "deck card id out of range");
        DeckCardId(i as u8)
    }

    #[inline]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// A multiset of one player's cards: for each `DeckCardId`, a 2-bit count of
/// how many copies are present, packed into one `u64` at bit `2 * deck_id`.
/// Used for every unordered zone — the hand, pitch zone, graveyard, banish
/// zones and worn equipment — since for an unordered zone the counts alone are
/// its contents.
///
/// The counts are untyped so that `Player` need not be generic over its deck.
/// The `_as` methods take a deck card enum directly.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub struct CardCounts(u64);

impl CardCounts {
    pub const EMPTY: CardCounts = CardCounts(0);

    #[inline]
    const fn shift(id: DeckCardId) -> u32 {
        (id.get() * BITS_PER_CARD) as u32
    }

    /// How many copies of the card `id` the zone holds.
    #[inline]
    pub const fn count(self, id: DeckCardId) -> u8 {
        ((self.0 >> Self::shift(id)) & MAX_COPIES as u64) as u8
    }

    /// Whether the zone holds at least one copy of the card `id`.
    #[inline]
    pub const fn contains(self, id: DeckCardId) -> bool {
        self.count(id) > 0
    }

    /// Add one copy of the card `id`. Panics if the zone already holds
    /// `MAX_COPIES` of it, since the count would carry into the neighbouring
    /// card's field.
    #[inline]
    pub fn add(&mut self, id: DeckCardId) {
        assert!(self.count(id) < MAX_COPIES, "too many copies of deck card {} in one zone", id.get());
        self.0 += 1 << Self::shift(id);
    }

    /// Remove one copy of the card `id`. Panics if the zone holds none, since
    /// the count would borrow from the neighbouring card's field.
    #[inline]
    pub fn remove(&mut self, id: DeckCardId) {
        assert!(self.contains(id), "no deck card {} in the zone to remove", id.get());
        self.0 -= 1 << Self::shift(id);
    }

    /// Total number of cards in the zone, counting every copy.
    #[inline]
    pub const fn len(self) -> u8 {
        ((self.0 & LOW_BITS).count_ones() + 2 * ((self.0 >> 1) & LOW_BITS).count_ones()) as u8
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The raw packed bits, e.g. for hashing or encoding an observation.
    #[inline]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Every card id with a non-zero count, lowest id first, with its count.
    #[inline]
    pub fn iter(self) -> impl Iterator<Item = (DeckCardId, u8)> {
        // One bit set at the low end of every non-empty field.
        let mut present = (self.0 | (self.0 >> 1)) & LOW_BITS;
        std::iter::from_fn(move || {
            if present == 0 {
                return None;
            }
            let shift = present.trailing_zeros();
            present &= present - 1;
            let id = DeckCardId::new(shift as usize / BITS_PER_CARD);
            Some((id, ((self.0 >> shift) & MAX_COPIES as u64) as u8))
        })
    }

    /// `count`, taking a card from deck `D`'s enum.
    #[inline]
    pub fn count_as<D: DeckCard>(self, card: D) -> u8 {
        self.count(card.id())
    }

    /// `iter`, yielding cards from deck `D`'s enum. The caller must know the
    /// counts belong to deck `D`; ids are not checked against the deck.
    #[inline]
    pub fn iter_as<D: DeckCard>(self) -> impl Iterator<Item = (D, u8)> {
        self.iter().map(|(id, n)| (D::from_id(id), n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: DeckCardId = DeckCardId::new(0);
    const B: DeckCardId = DeckCardId::new(5);
    /// The highest id: its field is the top two bits of the `u64`.
    const TOP: DeckCardId = DeckCardId::new(MAX_DECK_IDS - 1);

    #[test]
    fn add_remove_and_count() {
        let mut z = CardCounts::EMPTY;
        assert!(z.is_empty());
        z.add(B);
        z.add(B);
        z.add(A);
        z.add(TOP);
        assert_eq!(z.count(B), 2);
        assert_eq!(z.count(A), 1);
        assert_eq!(z.count(TOP), 1);
        assert_eq!(z.count(DeckCardId::new(1)), 0);
        assert_eq!(z.len(), 4);
        z.remove(B);
        assert_eq!(z.count(B), 1);
        assert_eq!(z.len(), 3);
    }

    #[test]
    fn three_copies_fit_without_touching_neighbours() {
        let mut z = CardCounts::EMPTY;
        for _ in 0..3 {
            z.add(B);
        }
        z.add(TOP);
        for _ in 0..3 {
            z.add(TOP.prev());
        }
        assert_eq!(z.count(B), 3);
        assert_eq!(z.count(DeckCardId::new(B.get() - 1)), 0);
        assert_eq!(z.count(DeckCardId::new(B.get() + 1)), 0);
        assert_eq!(z.count(TOP), 1);
        assert_eq!(z.len(), 7);
    }

    #[test]
    fn iter_yields_present_cards_in_id_order() {
        let mut z = CardCounts::EMPTY;
        z.add(TOP);
        z.add(B);
        z.add(B);
        z.add(A);
        let got: Vec<(usize, u8)> = z.iter().map(|(id, n)| (id.get(), n)).collect();
        assert_eq!(got, vec![(0, 1), (5, 2), (MAX_DECK_IDS - 1, 1)]);
        assert_eq!(CardCounts::EMPTY.iter().count(), 0);
    }

    #[test]
    fn typed_access_uses_the_deck_enum() {
        use crate::deck_cards::RhinarCard;
        let mut z = CardCounts::EMPTY;
        z.add(RhinarCard::DodgeB.id());
        z.add(RhinarCard::DodgeB.id());
        z.add(RhinarCard::AlphaRampageR.id());
        assert_eq!(z.count_as(RhinarCard::DodgeB), 2);
        assert_eq!(z.count_as(RhinarCard::PackHuntR), 0);
        let got: Vec<(RhinarCard, u8)> = z.iter_as().collect();
        assert_eq!(got, vec![(RhinarCard::AlphaRampageR, 1), (RhinarCard::DodgeB, 2)]);
    }

    #[test]
    #[should_panic(expected = "too many copies")]
    fn a_fourth_copy_panics() {
        let mut z = CardCounts::EMPTY;
        for _ in 0..4 {
            z.add(B);
        }
    }

    #[test]
    #[should_panic(expected = "no deck card 5")]
    fn removing_an_absent_card_panics() {
        let mut z = CardCounts::EMPTY;
        z.remove(B);
    }

    #[test]
    #[should_panic(expected = "deck card id out of range")]
    fn an_id_past_the_u64_panics() {
        DeckCardId::new(MAX_DECK_IDS);
    }

    impl DeckCardId {
        fn prev(self) -> Self {
            DeckCardId::new(self.get() - 1)
        }
    }
}
