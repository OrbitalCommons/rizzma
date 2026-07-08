//! Headless-browser tests for the wasm boundary: build a figure through the
//! JS surface, render it into a real `<canvas>`, read pixels back, and drive
//! the interaction session end-to-end with synthetic DOM events.
//!
//! Run with `wasm-pack test --headless --chrome crates/rizzma` (CI job
//! `wasm browser tests`); this file compiles to nothing on native targets.
#![cfg(target_arch = "wasm32")]

use rizzma::wasm::{WasmFigure, WasmSession};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, MouseEventInit, PointerEvent, PointerEventInit,
    WheelEvent, WheelEventInit,
};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// Create a `<canvas id=…>` pinned to the viewport origin (so synthetic
/// events' `clientX/Y` equal `offsetX/Y`) and append it to the body.
fn make_canvas(id: &str) -> HtmlCanvasElement {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.set_id(id);
    let style = canvas.style();
    style.set_property("position", "fixed").unwrap();
    style.set_property("left", "0px").unwrap();
    style.set_property("top", "0px").unwrap();
    document.body().unwrap().append_child(&canvas).unwrap();
    canvas
}

/// A 3x2 inch figure with one line on explicit limits, bound to `canvas_id`.
fn bound_session(canvas_id: &str) -> WasmSession {
    make_canvas(canvas_id);
    let mut fig = WasmFigure::new(3.0, 2.0);
    let ax = fig.add_axes(0.15, 0.15, 0.7, 0.7);
    fig.plot(ax, &[0.0, 5.0, 10.0], &[0.0, 5.0, 10.0]).unwrap();
    fig.set_xlim(ax, 0.0, 10.0).unwrap();
    fig.set_ylim(ax, 0.0, 10.0).unwrap();
    fig.bind(canvas_id).unwrap()
}

fn limits(session: &WasmSession) -> Vec<f64> {
    session.limits(0).unwrap().to_vec()
}

fn dispatch_pointer(canvas: &HtmlCanvasElement, kind: &str, x: f64, y: f64) {
    let init = PointerEventInit::new();
    init.set_client_x(x as i32);
    init.set_client_y(y as i32);
    init.set_button(0);
    init.set_pointer_id(1);
    init.set_bubbles(true);
    let ev = PointerEvent::new_with_event_init_dict(kind, &init).unwrap();
    canvas.dispatch_event(&ev).unwrap();
}

#[wasm_bindgen_test]
fn built_figure_renders_ink_into_the_canvas() {
    let canvas = make_canvas("render-target");
    let mut fig = WasmFigure::new(3.0, 2.0);
    let ax = fig.add_axes(0.15, 0.15, 0.7, 0.7);
    fig.plot(ax, &[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]).unwrap();
    fig.set_title(ax, "browser").unwrap();
    fig.render("render-target").unwrap();

    let (w, h) = (canvas.width(), canvas.height());
    assert!(w > 0 && h > 0, "canvas must have been sized by render()");

    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let data = context
        .get_image_data(0.0, 0.0, f64::from(w), f64::from(h))
        .unwrap()
        .data();
    assert_eq!(data.len(), (w as usize) * (h as usize) * 4);
    assert!(
        data.chunks_exact(4)
            .any(|px| px != [255, 255, 255, 255] && px[3] != 0),
        "expected non-white ink in the canvas readback"
    );
}

#[wasm_bindgen_test]
fn wheel_zoom_shrinks_limits_through_the_dom() {
    let session = bound_session("zoom-target");
    let before = limits(&session);

    let canvas: HtmlCanvasElement = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("zoom-target")
        .unwrap()
        .dyn_into()
        .unwrap();
    // One wheel detent up (deltaY = -120 pixels ≈ -1 line) at the canvas
    // center: zoom in.
    let init = WheelEventInit::new();
    init.set_client_x(150);
    init.set_client_y(100);
    init.set_delta_y(-120.0);
    init.set_delta_mode(WheelEvent::DOM_DELTA_PIXEL);
    init.set_cancelable(true);
    init.set_bubbles(true);
    let ev = WheelEvent::new_with_event_init_dict("wheel", &init).unwrap();
    canvas.dispatch_event(&ev).unwrap();

    let after = limits(&session);
    assert!(
        (after[1] - after[0]) < (before[1] - before[0]),
        "zoom in must shrink the x span: {before:?} -> {after:?}"
    );
    assert!(
        (after[3] - after[2]) < (before[3] - before[2]),
        "zoom in must shrink the y span: {before:?} -> {after:?}"
    );
}

#[wasm_bindgen_test]
fn drag_pan_and_double_click_reset_through_the_dom() {
    let session = bound_session("pan-target");
    let home = limits(&session);

    let canvas: HtmlCanvasElement = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("pan-target")
        .unwrap()
        .dyn_into()
        .unwrap();

    // Drag from the center 40 px right: the view pans (limits shift left).
    dispatch_pointer(&canvas, "pointerdown", 150.0, 100.0);
    dispatch_pointer(&canvas, "pointermove", 190.0, 100.0);
    dispatch_pointer(&canvas, "pointerup", 190.0, 100.0);

    let panned = limits(&session);
    assert!(
        panned[0] < home[0],
        "dragging right must pan toward smaller x: {home:?} -> {panned:?}"
    );
    let span = |l: &[f64]| l[1] - l[0];
    assert!(
        (span(&panned) - span(&home)).abs() < 1e-9,
        "pan must preserve the x span"
    );

    // Double-click restores the captured home limits.
    let init = MouseEventInit::new();
    init.set_client_x(150);
    init.set_client_y(100);
    init.set_bubbles(true);
    let ev = web_sys::MouseEvent::new_with_mouse_event_init_dict("dblclick", &init).unwrap();
    canvas.dispatch_event(&ev).unwrap();

    let reset = limits(&session);
    for (got, want) in reset.iter().zip(home.iter()) {
        assert!(
            (got - want).abs() < 1e-9,
            "double-click must restore home: {home:?} -> {reset:?}"
        );
    }
}

#[wasm_bindgen_test]
fn live_data_updates_repaint_and_preserve_the_view() {
    let session = bound_session("live-target");
    let home = limits(&session);

    let canvas: HtmlCanvasElement = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("live-target")
        .unwrap()
        .dyn_into()
        .unwrap();
    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let (w, h) = (canvas.width(), canvas.height());
    let before = context
        .get_image_data(0.0, 0.0, f64::from(w), f64::from(h))
        .unwrap()
        .data();

    // Replace the line with a very different shape and paint synchronously.
    session
        .set_line_data(0, 0, &[0.0, 5.0, 10.0], &[9.0, 0.5, 9.0])
        .unwrap();
    session.render().unwrap();

    let after = context
        .get_image_data(0.0, 0.0, f64::from(w), f64::from(h))
        .unwrap()
        .data();
    assert_ne!(
        before.to_vec(),
        after.to_vec(),
        "new data must repaint different pixels"
    );

    // The explicit limits (the framed view) survive the data swap.
    assert_eq!(limits(&session), home);

    // Bad indices surface as errors, not panics.
    assert!(session.set_line_data(0, 9, &[0.0], &[0.0]).is_err());
}
