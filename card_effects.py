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
    ON_DISCARD = auto()   # a card is discarded from hand during the action phase
    ON_ATTACK  = auto()   # fired when an attack card is declared (before defend step)
    ON_PLAY    = auto()   # fired when a non-attack action card is played


class EffectAction(Enum):
    """Actions executed when a matching effect fires."""
    INTIMIDATE = auto()   # opponent banishes a random card from hand until end of turn


@dataclass
class CardEffect:
    """A triggered ability defined on a card.

    Attributes
    ----------
    trigger:
        The event that causes this effect to fire.
    action:
        What happens when it fires.
    condition:
        Optional callable that receives the event context dict and returns
        True if the effect should fire.  When absent, the effect fires on
        every occurrence of *trigger*.
    """

    trigger: EffectTrigger
    action: EffectAction
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
