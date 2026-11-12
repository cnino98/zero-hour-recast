"""Pinned Zero Hour numeric macros for extracted C++ reference checks."""
from pathlib import Path
import reference_support

HEADER = "Core/Libraries/Include/Lib/BaseFunctions.h"
PROJECT = Path(__file__).resolve().parents[2]


def function(source, signature):
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def cpp(reference):
    if reference.run(["git", "-C", str(reference.UPSTREAM), "status", "--porcelain", "--", HEADER]):
        raise RuntimeError("Modified reference numeric header")
    source = (reference.UPSTREAM / HEADER).read_text()
    parts = [source.split("*/", 1)[0] + "*/", "#include <cmath>",
             "#define __forceinline inline", "#define RTS_GENERALS 0"]
    for signature in ["__forceinline long fast_float2long_round(",
                      "__forceinline float fast_float_trunc(",
                      "__forceinline float fast_float_floor(",
                      "__forceinline float fast_float_ceil("]:
        parts.append(function(source, signature))
    start = source.index("#if RTS_GENERALS")
    end = source.index("#endif", start) + len("#endif")
    parts.append(source[start:end])
    return "\n".join(parts) + "\n"


def rust():
    source = (PROJECT / "crates/generals-sim/src/native_real.rs").read_text()
    source = source.split("#[cfg(test)]", 1)[0]
    # The extraction keeps the actual production functions. Only num_traits'
    # checked primitive cast boundary is supplied for standalone rustc.
    adapter = """
trait ToPrimitive { fn to_i32(self) -> Option<i32>; }
impl ToPrimitive for f32 {
    fn to_i32(self) -> Option<i32> {
        if self >= -2147483648.0 && self < 2147483648.0 { Some(self as i32) } else { None }
    }
}
"""
    return "pub mod native_real {\n" + source.replace("use num_traits::ToPrimitive;", adapter) + "\n}\n"
