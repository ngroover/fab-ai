
import cards

cards.CARD_CATALOG['bare_fangs_red'] = Card("Bare Fangs", CardType.ACTION_ATTACK, cost=2, pitch=1,
                           power=6, defense=0, color=Color.RED, no_block=True,
                           card_class=CardClass.BRUTE,
                           text="When you attack with Bare Fangs, draw a card then discard a random card. If a card wth 6 or more power is discarded this way, Bare Fangs gets +2 power.",
                           effects=[CardEffect(trigger=EffectTrigger.ON_ATTACK, action=EffectAction.DRAW_DISCARD_POWER_BONUS)])
