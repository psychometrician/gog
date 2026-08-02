# hatch_build.py — put the engine in the wheel
#
# The Python package is a bridge to a Rust binary, so an installed copy that
# cannot find that binary cannot draw anything. That was the blocker on releasing
# either binding, and the decision taken
# was to **ship the binary**: one wheel per platform, each carrying the
# `gog-cli` built for it, so `pip install gog` works with no Rust toolchain in
# sight.
#
# What that means mechanically is two lines of build data. The binary is force-
# included at `gog/_bin/`, where `render.py` looks first; and the wheel is
# marked platform-specific, because a wheel carrying a macOS arm64 executable
# must not be handed to a Linux box.
#
# The binary is not built here. CI builds it once per platform and this packs
# it, which keeps `cargo` out of the install path for a user and out of the
# build path for anyone who already has a release build.

import os
import sysconfig

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

EXE = "gog-cli.exe" if os.name == "nt" else "gog-cli"


def _engine(root: str) -> str:
    """The compiled engine, or a message saying how to get one."""
    override = os.environ.get("GOG_CLI_PATH", "")
    if override and os.path.isfile(override):
        return override

    for build in ("release", "debug"):
        candidate = os.path.join(root, "..", "..", "target", build, EXE)
        if os.path.isfile(candidate):
            return os.path.abspath(candidate)

    raise RuntimeError(
        "gog: the wheel carries the engine, and there is no engine to carry.\n"
        "  cargo build --release -p gog-cli\n"
        "Or point GOG_CLI_PATH at one built elsewhere."
    )


def _browser_engine(root: str):
    """`(source, target)` for the WebAssembly engine and its runtime, or nothing.

    Returns an empty list when either is missing, and that is not an error: the
    wheel is complete without them and a 3-D plot simply stays still. Contrast
    `_engine` above, which raises, because a wheel with no `gog-cli` cannot draw
    at all.

    `GOG_WASM_PATH` overrides the search the way `GOG_CLI_PATH` does, so a
    release job that built the module somewhere else can say where.
    """
    wasm = os.environ.get("GOG_WASM_PATH", "") or os.path.join(
        root, "..", "..", "gog-wasm", "target", "wasm32-unknown-unknown",
        "release", "gog_wasm.wasm",
    )
    src = os.path.join(root, "..", "..", "js-pkg", "gog", "src")
    js = os.path.join(src, "interactive.js")
    view = os.path.join(src, "view.js")
    out = []
    # **The view module ships on its own terms**, because it needs no engine: it
    # carries zoom, pan and fit for every plot, and pairing it with the wasm would
    # mean a wheel built without WebAssembly shipped plots that cannot be looked
    # at closely.
    if os.path.isfile(view):
        out.append((os.path.abspath(view), "gog/_www/view.js"))
    if os.path.isfile(wasm) and os.path.isfile(js):
        out += [
            (os.path.abspath(wasm), "gog/_www/gog.wasm"),
            (os.path.abspath(js), "gog/_www/interactive.js"),
        ]
    return out


def _platform_tag() -> str:
    """The platform this wheel is for.

    CI sets `GOG_WHEEL_PLAT` per matrix entry, because the tag a release needs
    is not always the one the build machine reports: a Linux wheel must claim a
    `manylinux` tag to be installable from PyPI, and a macOS wheel is built
    against a deployment target rather than the runner's own version.
    """
    declared = os.environ.get("GOG_WHEEL_PLAT", "")
    if declared:
        return declared
    return sysconfig.get_platform().replace("-", "_").replace(".", "_")


class CustomBuildHook(BuildHookInterface):
    PLUGIN_NAME = "custom"

    def initialize(self, version, build_data):
        if self.target_name != "wheel":
            return  # an sdist carries source, and a binary is not source

        build_data["force_include"][_engine(self.root)] = f"gog/_bin/{EXE}"

        # The browser engine rides along when it has been built, and the wheel is
        # complete without it. A 3-D plot in a notebook can be turned with the
        # mouse, which needs the engine compiled to WebAssembly plus the module
        # that drives it; absent them, the plot is the still picture it always
        # was, which is also what a JavaScript-less viewer and a PDF get.
        #
        # Optional rather than required, unlike `_engine` above, because the two
        # fail differently: without `gog-cli` nothing draws at all, while without
        # this a plot draws and does not turn. Making it required would mean a
        # `wasm32-unknown-unknown` target on every machine that builds a wheel,
        # to gate a feature many users never reach.
        #
        # One file covers every platform. WebAssembly is portable, so this is the
        # same bytes in all five wheels — no matrix, and nothing to pick wrong.
        for source, target in _browser_engine(self.root):
            build_data["force_include"][source] = target

        # Pure-Python wheels are tagged `py3-none-any` and installed anywhere,
        # which is exactly wrong for one holding a native executable.
        build_data["pure_python"] = False
        build_data["tag"] = f"py3-none-{_platform_tag()}"
