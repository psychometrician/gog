//! gog-wasm — the browser bridge.
//!
//! `gog-cli` reads a `RenderRequest` from stdin and writes SVG to stdout. This
//! reads the identical JSON from a pointer into linear memory and writes the
//! SVG back the same way. **Only the transport differs**: the decoding, the
//! missing-value policy, the legality gate and the renderer are all
//! `gog-core`'s, so a plot drawn in a browser is the same plot the command line
//! draws — which is checked, not assumed, by a test comparing the two byte for
//! byte.
//!
//! Why a second bridge exists at all: a subprocess cannot run in a web page, and
//! the page is where a 3-D plot becomes turnable. The engine renders a cube at
//! any viewing angle already, so dragging is the same spec re-rendered with two
//! numbers changed — which needs the engine *in* the page, at frame rate, rather
//! than a process spawn per frame.
//!
//! # The interface
//!
//! Three functions over a byte buffer, and no `wasm-bindgen`:
//!
//! - [`alloc`] — get `n` bytes to write a request into.
//! - [`gog_render`] — render; returns a pointer, and writes the length and a
//!   status through out-parameters.
//! - [`dealloc`] — hand a buffer back.
//!
//! # Ownership, which is the only sharp edge
//!
//! [`gog_render`] **consumes** the request buffer — do not free it afterward.
//! The buffer it returns is the caller's to free, and must be freed: a dragged
//! frame allocates its request and its SVG, roughly 240 KB a frame, so a minute
//! of dragging at 60 fps leaks most of a gigabyte if the caller does not. That
//! is not a hypothetical; it showed up as jank in the prototype and was briefly
//! mistaken for a DOM cost.

use gog_core::wire::{self, RenderRequest};

/// Rendered; the returned buffer is SVG.
pub const STATUS_OK: i32 = 0;
/// The JSON did not parse; the buffer is the parser's message.
pub const STATUS_BAD_JSON: i32 = 1;
/// The plot was refused; the buffer is the diagnostics, one per line, and
/// nothing was drawn. Mirrors `gog-cli` exiting 2 — the same policy, a
/// different transport.
pub const STATUS_REFUSED: i32 = 2;

/// Reserve `n` bytes and hand back a pointer to them.
///
/// # Safety
/// The returned pointer is valid for `n` bytes and must be released by
/// [`dealloc`], or consumed by [`gog_render`].
#[no_mangle]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut v: Vec<u8> = Vec::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Release a buffer obtained from [`alloc`] or returned by [`gog_render`].
///
/// # Safety
/// `ptr` must have come from this module and `cap` must be the capacity it was
/// created with. Passing a foreign pointer, or freeing twice, is undefined.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, cap: usize) {
    if !ptr.is_null() && cap > 0 {
        drop(Vec::from_raw_parts(ptr, 0, cap));
    }
}

/// Render a request.
///
/// Reads `len` bytes of UTF-8 JSON at `ptr`, and writes the result's length
/// through `out_len` and one of the `STATUS_*` constants through `out_status`.
/// Returns a pointer to the result — SVG when the status is [`STATUS_OK`], and
/// the message text otherwise.
///
/// Non-fatal diagnostics — an Assumption the engine made, or a count of rows a
/// missing value cost — ride along on a **successful** render and would
/// otherwise be lost, since a browser has no stderr to print them to. They are
/// held for [`gog_notes`] to collect. Never dropping a diagnostic in silence is
/// the rule this obeys.
///
/// # Safety
/// `ptr` must point at `len` readable bytes from [`alloc`]; it is **consumed**.
/// `out_len` and `out_status` must be writable. The returned buffer belongs to
/// the caller and must be released with [`dealloc`].
#[no_mangle]
pub unsafe extern "C" fn gog_render(
    ptr: *mut u8,
    len: usize,
    out_len: *mut usize,
    out_status: *mut i32,
) -> *mut u8 {
    let input = String::from_raw_parts(ptr, len, len);

    let (text, status) = render_to_string(&input);

    let mut bytes = text.into_bytes();
    bytes.shrink_to_fit();
    *out_len = bytes.len();
    *out_status = status;
    let p = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    p
}

/// Collect the notes from the last successful [`gog_render`], one per line, and
/// clear them. Returns a pointer; writes the length through `out_len`. An empty
/// result means the render had nothing to say.
///
/// # Safety
/// `out_len` must be writable. The returned buffer must be released with
/// [`dealloc`].
#[no_mangle]
pub unsafe extern "C" fn gog_notes(out_len: *mut usize) -> *mut u8 {
    let text = NOTES.with(|n| std::mem::take(&mut *n.borrow_mut()));
    let mut bytes = text.into_bytes();
    bytes.shrink_to_fit();
    *out_len = bytes.len();
    let p = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    p
}

thread_local! {
    /// Notes from the last render. A thread-local rather than a `static mut`
    /// because it is the safe spelling of the same thing, and WebAssembly's
    /// single thread makes the distinction free.
    static NOTES: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The whole of the bridge's logic, split out so a test can call it directly
/// rather than through raw pointers.
fn render_to_string(input: &str) -> (String, i32) {
    let request: RenderRequest = match serde_json::from_str(input) {
        Ok(r) => r,
        Err(e) => return (format!("gog: JSON parse error: {e}"), STATUS_BAD_JSON),
    };

    // `decode` consumes the tables, so the spec is taken first.
    let spec = request.spec.clone();
    let (data, remarks) = wire::decode(request);

    match gog_core::plot::render_figure(&spec, &data) {
        Ok(drawing) => {
            let notes: Vec<String> = remarks
                .into_iter()
                .chain(drawing.diagnostics.iter().map(|d| d.message.clone()))
                .collect();
            if !notes.is_empty() {
                NOTES.with(|n| *n.borrow_mut() = notes.join("\n"));
            }
            (drawing.svg, STATUS_OK)
        }
        Err(diagnostics) => (
            diagnostics
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("\n"),
            STATUS_REFUSED,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: &str = r#"{
        "spec": {"data":"t","layers":[{"mark":"point","encodings":{
            "x":{"field":"a"},"y":{"field":"b"},"z":{"field":"c"}},"transforms":[]}],
            "coord":{"space":{"turn":45,"tilt":25}}},
        "data": {"t": {"floats": {"a":[1.0,2.0,3.0],"b":[2.0,1.0,3.0],"c":[3.0,2.0,1.0]}}}
    }"#;

    #[test]
    fn a_cube_renders_to_svg() {
        let (svg, status) = render_to_string(CUBE);
        assert_eq!(status, STATUS_OK, "{svg}");
        assert!(svg.starts_with("<svg"), "{}", &svg[..60.min(svg.len())]);
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    /// Turning the cube must actually re-project it. If this ever passes with
    /// identical output, drag would move the mouse and nothing else.
    #[test]
    fn a_different_turn_draws_a_different_picture() {
        let (a, _) = render_to_string(CUBE);
        let (b, _) = render_to_string(&CUBE.replace("\"turn\":45", "\"turn\":90"));
        assert_ne!(a, b, "the viewing angle must change the projection");
    }

    /// A refusal comes back as a status and its diagnostics, never as a broken
    /// picture — the browser's spelling of the CLI exiting 2 and drawing
    /// nothing.
    #[test]
    fn a_refused_plot_returns_its_diagnostics_and_no_svg() {
        // `line` with a `z` is the decided refusal: a cube has no left to right.
        let bad = CUBE.replace("\"mark\":\"point\"", "\"mark\":\"line\"");
        let (text, status) = render_to_string(&bad);
        assert_eq!(status, STATUS_REFUSED, "got: {text}");
        assert!(!text.contains("<svg"), "nothing may be drawn: {text}");
        assert!(!text.is_empty(), "a refusal must say why");
    }

    #[test]
    fn malformed_json_is_reported_rather_than_panicking() {
        let (text, status) = render_to_string("{not json");
        assert_eq!(status, STATUS_BAD_JSON);
        assert!(text.contains("parse error"), "{text}");
    }
}
