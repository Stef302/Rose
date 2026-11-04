"""Rose v2 — compilateur multi-cibles pour le langage de mathématiques Rose."""
from .lexer import ErreurRose, tokeniser
from .parser import analyser
from .checker import Verificateur, replier_constants
from .codegen_c import generer_c
from .codegen_python import generer_python
from .codegen_julia import generer_julia
from .codegen_rust import generer_rust
from .codegen_zig import generer_zig

__version__ = "2.0.0"
__all__ = ["ErreurRose", "tokeniser", "analyser", "Verificateur", "replier_constants",
           "generer_c", "generer_python", "generer_julia", "generer_rust", "generer_zig"]
