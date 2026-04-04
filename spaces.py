"""
Minimal Space implementations matching the gymnasium API.
Drop-in compatible with gymnasium.spaces — swap the import once gymnasium is available.
"""
from __future__ import annotations
import random
from typing import Any, Dict as DictType, List, Optional, Tuple


class Space:
    """Abstract base class."""
    def sample(self) -> Any:
        raise NotImplementedError
    def contains(self, x: Any) -> bool:
        raise NotImplementedError


class Discrete(Space):
    """Integer actions in [0, n)."""
    def __init__(self, n: int):
        self.n = n

    def sample(self) -> int:
        return random.randrange(self.n)

    def contains(self, x: Any) -> bool:
        return isinstance(x, int) and 0 <= x < self.n

    def __repr__(self):
        return f"Discrete({self.n})"


class Box(Space):
    """Continuous/integer box. shape is a tuple of ints."""
    def __init__(self, low: float, high: float, shape: Tuple[int, ...], dtype=float):
        self.low = low
        self.high = high
        self.shape = shape
        self.dtype = dtype

    def sample(self) -> List:
        import random
        flat = [random.uniform(self.low, self.high) for _ in range(
            1 if not self.shape else self.shape[0])]
        return flat

    def contains(self, x: Any) -> bool:
        return True  # permissive for now

    def __repr__(self):
        return f"Box({self.low}, {self.high}, shape={self.shape}, dtype={self.dtype.__name__})"


class Dict(Space):
    """Dictionary of named sub-spaces."""
    def __init__(self, spaces: DictType[str, Space]):
        self.spaces = spaces

    def sample(self) -> DictType[str, Any]:
        return {k: v.sample() for k, v in self.spaces.items()}

    def contains(self, x: Any) -> bool:
        if not isinstance(x, dict):
            return False
        return all(k in x and self.spaces[k].contains(x[k]) for k in self.spaces)

    def __repr__(self):
        inner = ", ".join(f"{k}: {v}" for k, v in self.spaces.items())
        return f"Dict({{{inner}}})"
