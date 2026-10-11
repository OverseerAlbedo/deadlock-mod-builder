"""Optional model preparation. This package never compiles or installs a mod."""
from dataclasses import dataclass, field
from typing import Protocol

MODES = ('prepared', 'rigged_humanoid', 'unrigged_humanoid')

@dataclass
class PreparationResult:
    dmx: str
    blend: str
    report: str
    textures: str
    target_model: str
    warnings: list[str] = field(default_factory=list)

class HumanoidPreparationBackend(Protocol):
    def inspect(self, source: str, destination, **options) -> dict: ...
    def prepare(self, request: dict, destination) -> dict: ...
