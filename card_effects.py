"""
CardEffect system for Flesh and Blood TCG simulation.

Effects are attached to Card instances and collected by Players at game start.
FaBEnv fires effects generically via EffectTrigger events — no card-specific
logic lives in the environment.

Usage
-----
Define an effect on a card::

    CardEffect(
        trigger=EffectTrigger.ON_DISCARD,
        action=EffectAction.INTIMIDATE,
        condition=lambda ctx: ctx.get("card") is not None and ctx["card"].power >= 6,
    )

The environment fires it::

    env._fire_effects(EffectTrigger.ON_DISCARD, {"card": discarded_card}, player, opponent)
"""

from enum import Enum, auto
from dataclasses import dataclass, field
from typing import Optional, Callable, Any, Dict


class EffectTrigger(Enum):
    """Events that can activate a card effect."""
    ON_DISCARD         = auto()   # a card is discarded from hand during the action phase
    ON_ATTACK          = auto()   # fired when an attack card is declared (before defend step)
    ON_PLAY            = auto()   # fired when a non-attack action card is played
    ON_ATTACK_REACTION = auto()   # fired when this card resolves as an attack reaction
    ON_DEFEND          = auto()   # fired when this card is used to defend


class EffectAction(Enum):
    """Actions executed when a matching effect fires."""
    INTIMIDATE               = auto()   # opponent banishes a random card from hand until end of turn
    DRAW_DISCARD_GO_AGAIN    = auto()   # draw a card, discard random; if 6+ power discarded → go again
    DRAW_DISCARD_POWER_BONUS = auto()   # draw a card, discard random; if 6+ power discarded → +2 power
    DRAW_DISCARD_INTIMIDATE  = auto()   # draw a card, discard random; if 6+ power discarded → intimidate
    ATTACK_POWER_BOOST             = auto()   # target attack gains +magnitude power (see CardEffect.magnitude)
    SWORD_ATTACK_GO_AGAIN          = auto()   # target sword attack gains go again
    NEXT_SWORD_ATTACK_POWER_BONUS  = auto()   # next sword attack this turn gains +magnitude power
    WEAPON_ATTACK_POWER_BONUS          = auto()   # if weapon was attacked this turn, next attack gains +magnitude power
    WEAPON_ATTACK_BONUS_PER_SWING      = auto()   # 1st weapon attack this turn +1, 2nd weapon attack this turn +2
    REVEAL_TOP_DECK_POWER_CHECK        = auto()   # reveal top card of deck; if 6+ power keep on top, else move to bottom


@dataclass
class CardEffect:
    """A triggered ability defined on a card.

    Attributes
    ----------
    trigger:
        The event that causes this effect to fire.
    action:
        What happens when it fires.
    magnitude:
        Numeric parameter used by actions such as ATTACK_POWER_BOOST.
    condition:
        Optional callable that receives the event context dict and returns
        True if the effect should fire.  When absent, the effect fires on
        every occurrence of *trigger*.
    """

    trigger: EffectTrigger
    action: EffectAction
    magnitude: int = 0
    condition: Optional[Callable[[Dict[str, Any]], bool]] = field(
        default=None, compare=False
    )

    def matches(self, trigger: EffectTrigger, context: Dict[str, Any]) -> bool:
        """Return True when this effect should fire for *trigger* / *context*."""
        if self.trigger != trigger:
            return False
        if self.condition is not None:
            return self.condition(context)
        return True
