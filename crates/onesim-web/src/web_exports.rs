use crate::rn::{deliver, deliver_coalesced, queue_scroll_events, rn_pointer, with_rn};
use crate::state::{
    dispatch, push_environment, send_layout, set_err, submit, sync_guest_clock,
    update_animation_frame, update_live_frame, update_scene, with,
};
#[cfg(all(target_family = "wasm", not(feature = "js-guest")))]
use crate::state::__main_argc_argv;
#[cfg(all(target_family = "wasm", feature = "js-guest"))]
use crate::state::on_init;
use onesim_protocol::{DynamicTypeSize, HostMessage, IosVersion, KeyboardEvent, PointerEvent, PointerKind, ScenePhase};
use onesim_render::{device, resolve_label, DragPhase, Effect, DEVICES};
use std::ffi::{c_void, CString};

/// Device list as JSON `[{name,width,height,scale,safeTop,safeBottom,android}]` in the scratch buffer (usable before init).
#[no_mangle]
pub extern "C" fn simcore_web_devices() -> i32 {
    let list: Vec<_> = DEVICES
        .iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "width": d.viewport.width,
                "height": d.viewport.height,
                "scale": d.scale,
                "safeTop": d.safe_top,
                "safeBottom": d.safe_bottom,
                "android": d.android,
            })
        })
        .collect();
    with(|s| s.buf = serde_json::to_vec(&list).unwrap_or_default());
    0
}

/// Select the device whose name is the `len` UTF-8 bytes in the scratch buffer (chrome only; the
/// viewport and scale come from `simcore_web_init`). Call before init. 0 ok, 1 unknown name.
#[no_mangle]
pub extern "C" fn simcore_web_set_device(len: usize) -> i32 {
    with(|s| match device(&String::from_utf8_lossy(&s.buf[..len.min(s.buf.len())])) {
        Some(d) => {
            s.device = d;
            0
        }
        None => 1,
    })
}

/// Submit a ViewNode JSON tree written into the scratch buffer (`simcore_web_buf`), for JS-hosted guests.
/// Returns 0 on success, 1 on error.
#[no_mangle]
pub extern "C" fn simcore_web_submit_tree(len: usize, version: f64) -> i32 {
    let buf = with(|s| std::mem::take(&mut s.buf));
    let res = submit(&buf[..len.min(buf.len())], version.max(0.0) as u64);
    with(|s| {
        if s.buf.is_empty() {
            s.buf = buf;
        }
    });
    match res {
        Ok(()) => 0,
        Err(e) => {
            set_err(e);
            1
        }
    }
}

/// Evaluate the app and render the first frame. Call once, after `_initialize`.
/// `width` x `height` = viewport in points; `scale` = device pixels per point.
#[no_mangle]
pub extern "C" fn simcore_web_init(width: f64, height: f64, scale: f64, dark: i32) -> i32 {
    let (w, h, safe_top, safe_bottom) = with(|s| {
        let w = width.max(1.0);
        let h = height.max(1.0);
        s.viewport = (w, h);
        s.scale = scale.clamp(1.0, 4.0);
        s.dark = dark != 0;
        s.initialized = true;
        (w, h, s.device.safe_top, s.device.safe_bottom)
    });
    #[cfg(all(target_family = "wasm", feature = "js-guest"))]
    {
        unsafe { on_init(if dark != 0 { 1 } else { 0 }, w, h, safe_top, safe_bottom) }
    }
    #[cfg(not(all(target_family = "wasm", feature = "js-guest")))]
    {
        let _ = (w, h, safe_top, safe_bottom);
        #[cfg(target_family = "wasm")]
        unsafe {
            __main_argc_argv(0, std::ptr::null());
        }
        push_environment()
    }
}

#[no_mangle]
pub extern "C" fn simcore_web_set_dark(dark: i32) -> i32 {
    with(|s| s.dark = dark != 0);
    push_environment()
}

/// Dynamic Type size as an index into `DynamicTypeSize::ALL` (0 = xSmall, 3 = large, 11 = accessibility5).
#[no_mangle]
pub extern "C" fn simcore_web_set_dynamic_type(index: i32) -> i32 {
    with(|s| s.dynamic_type = DynamicTypeSize::from_index(index.max(0) as usize));
    // Before `init` there is no guest yet; the size is picked up by the first render.
    if with(|s| s.on_env.is_some() || s.initialized || s.rn.is_some()) {
        push_environment()
    } else {
        0
    }
}

/// SwiftUI scene phase: 0 active, 1 inactive, 2 background (e.g. the page became hidden or visible). Steps
/// through `inactive` as iOS does; each step goes to the guest as a `scene_phase` line, which answers with a tree.
/// Returns 0, or -1 for an unknown phase.
#[no_mangle]
pub extern "C" fn simcore_web_set_scene_phase(phase: i32) -> i32 {
    let Some(&to) = usize::try_from(phase).ok().and_then(|i| ScenePhase::ALL.get(i)) else { return -1 };
    if with(|s| s.scene_phase) == ScenePhase::Active && to != ScenePhase::Active {
        // iOS cancels the app's touches as it resigns active.
        if let Some(e) = with(|s| s.scene.as_ref().map(|sc| s.inter.cancel_touches(sc))) {
            dispatch(e);
        }
    }
    for step in with(|s| s.scene_phase.steps_to(to)) {
        let (ctx, cb) = with(|s| {
            s.scene_phase = step;
            (s.ctx, s.on_layout)
        });
        let line = serde_json::to_string(&HostMessage::ScenePhase { phase: step }).expect("serialize");
        if let (Some(cb), Ok(msg)) = (cb, CString::new(line)) {
            unsafe { cb(ctx as *mut c_void, msg.as_ptr()) };
        }
    }
    0
}

/// The scene phase as `simcore_web_set_scene_phase` numbers it.
#[no_mangle]
pub extern "C" fn simcore_web_scene_phase() -> i32 {
    let p = with(|s| s.scene_phase);
    ScenePhase::ALL.iter().position(|&q| q == p).unwrap_or(0) as i32
}

/// Simulated iOS release by index into `IosVersion::ALL` (0 = 26.5, 1 = 27.2); out of range = newest.
#[no_mangle]
pub extern "C" fn simcore_web_set_ios(index: i32) -> i32 {
    with(|s| {
        s.ios = IosVersion::ALL
            .get(index.max(0) as usize)
            .copied()
            .unwrap_or_default();
        update_scene(s);
    });
    0
}

#[no_mangle]
pub extern "C" fn simcore_web_set_viewport(width: f64, height: f64) -> i32 {
    with(|s| s.viewport = (width.max(1.0), height.max(1.0)));
    push_environment()
}

/// Re-render at another device pixel scale (e.g. the window resized); no guest call.
#[no_mangle]
pub extern "C" fn simcore_web_set_scale(scale: f64) {
    with(|s| {
        s.scale = scale.clamp(1.0, 4.0);
        update_scene(s);
    });
}

/// kind: 0 down, 1 up, 2 click. x, y in points. Returns 1 if the interaction changed anything.
#[no_mangle]
pub extern "C" fn simcore_web_pointer(kind: i32, x: f64, y: f64) -> i32 {
    let kind = match kind {
        0 => PointerKind::Down,
        1 => PointerKind::Up,
        3 => PointerKind::DoubleClick,
        4 => PointerKind::LongPress,
        _ => PointerKind::Click,
    };
    rn_pointer(PointerEvent::new(kind, x, y)).unwrap_or_else(|| {
        sync_guest_clock();
        let e = with(|s| s.scene.as_ref().map(|sc| s.inter.pointer(sc, PointerEvent::new(kind, x, y))));
        e.map_or(0, dispatch)
    })
}

/// Scroll the scroller under (x, y) by (dx, dy) points; positive dy reveals later content.
#[no_mangle]
pub extern "C" fn simcore_web_scroll(x: f64, y: f64, dx: f64, dy: f64) -> i32 {
    let ev = PointerEvent::moved(PointerKind::Scroll, x, y, dx, dy);
    if let Some(r) = rn_pointer(ev) {
        return r;
    }
    let e = with(|s| s.scene.as_ref().map(|sc| s.inter.pointer(sc, ev)));
    e.map_or(0, dispatch)
}

/// Touch drag (finger scrolling). phase: 0 begin (at the touch-down point), 1 move, 2 end (lift),
/// 3 coalesced move (defers scene rebuild), 4 cancel (lost pointer capture).
/// x, y in points; `t` is the event time in ms. Returns 1 if the frame changed. After an end,
/// call `simcore_web_tick` every frame while it returns 1 (momentum, bounce).
#[no_mangle]
pub extern "C" fn simcore_web_drag(phase: i32, x: f64, y: f64, t: f64) -> i32 {
    let coalesced = phase == 3;
    let cancelled = phase == 4;
    let phase = match phase {
        0 => DragPhase::Begin,
        1 | 3 => DragPhase::Move,
        _ => DragPhase::End,
    };
    if let Some(out) = with_rn(|rn, inter, cx| rn.drag(inter, cx, phase, x, y, t)) {
        return if coalesced { deliver_coalesced(out) } else { deliver(out) };
    }
    sync_guest_clock();
    let e = with(|s| s.scene.as_ref().map(|sc| {
        if cancelled { s.inter.cancel_touches(sc) } else { s.inter.drag(sc, phase, x, y, t) }
    }));
    if coalesced {
        return match e {
            Some(Effect::None) | None => 0,
            Some(Effect::Redraw) => 1,
            Some(other) => {
                // Deliver every coalesced sample to the guest so recognizers observe threshold
                // crossings (e.g. moving past drag slop and back within one frame), while
                // deferring layout/scene rebuilds until the frame's final move.
                with(|s| s.defer_scene = true);
                let rc = dispatch(other);
                with(|s| s.defer_scene = false);
                rc
            }
        };
    }
    let rc = e.map_or(0, dispatch);
    if with(|s| s.scene_dirty) {
        with(|s| {
            s.scene_dirty = false;
            update_scene(s);
        });
        send_layout();
        return 1;
    }
    rc
}

/// Advance scroll animations, SwiftUI animations, and scheduled guest ticks by `dt_ms`.
/// Returns 1 while anything is still active (redrawn); 0 = at rest.
#[no_mangle]
pub extern "C" fn simcore_web_tick(dt_ms: f64) -> i32 {
    let dt_ms = dt_ms.max(0.0);
    let dt = dt_ms / 1000.0;
    let tick_guest = with(|s| {
        s.clock_ms += dt_ms;
        if s.guest_due_ms.is_some_and(|due| s.clock_ms + 1e-6 >= due) {
            s.guest_due_ms = None;
            s.guest_clock_ms = s.clock_ms;
            Some((s.ctx, s.on_layout, s.clock_ms))
        } else {
            None
        }
    });
    if let Some((ctx, Some(cb), time)) = tick_guest {
        if let Ok(msg) = CString::new(format!(r#"{{"type":"tick","time":{time}}}"#)) {
            unsafe { cb(ctx as *mut c_void, msg.as_ptr()) };
        }
    }
    let redrew = with(|s| {
        let scroll_moved = s.scene.as_ref().is_some_and(|sc| s.inter.tick(sc, dt));
        queue_scroll_events(s);
        let anim_moved = dt > 0.0 && s.anim.is_animating();
        if anim_moved {
            s.anim.tick(dt);
        }
        let live_moved = if scroll_moved {
            update_scene(s);
            false
        } else if anim_moved {
            update_animation_frame(s);
            false
        } else {
            dt > 0.0 && update_live_frame(s)
        };
        scroll_moved || anim_moved || live_moved
    });
    if redrew {
        send_layout();
    }
    // A finger resting on a gesture view keeps reporting (long presses fire while held).
    let held = with(|s| s.inter.holding());
    if held {
        dispatch(with(|s| s.inter.hold(dt)));
    }
    let more = with(|s| s.anim.is_animating() || s.guest_due_ms.is_some() || s.live_moving);
    // Return 1 if we redrew (including the final settling frame) so the browser presents it; the next tick returns 0.
    (redrew || held || more || tick_guest.is_some()) as i32
}

/// Keyboard input for the focused field; the payload is `len` UTF-8 bytes in the scratch buffer.
/// kind 0: text to append (programmatic, like native `--type`: shift untouched); kind 1: key name
/// (`backspace`, `enter`, or `back` for Android system back); kind 2: text typed by a person on a physical keyboard (shift then
/// follows iOS auto-capitalization, as when tapping keys). Returns 1 if applied,
/// 0 if ignored, -1 if no field is focused.
#[no_mangle]
pub extern "C" fn simcore_web_key(kind: i32, len: usize) -> i32 {
    let payload = with(|s| String::from_utf8_lossy(&s.buf[..len.min(s.buf.len())]).into_owned());
    let ev = match (kind, payload.as_str()) {
        (0 | 2, _) => KeyboardEvent::Text { text: payload },
        (1, "backspace" | "enter" | "back") => KeyboardEvent::Key { key: payload },
        _ => return 0,
    };
    if with(|s| s.rn.is_some()) {
        return match with_rn(|rn, inter, cx| rn.keyboard(inter, cx, ev)) {
            Some(Ok(out)) => deliver(out),
            _ => -1,
        };
    }
    sync_guest_clock();
    let e = with(|s| {
        let sc = s.scene.as_ref().ok_or("no scene")?;
        let r = s.inter.keyboard(sc, ev);
        if let (2, Ok(Effect::TextInput { text, .. })) = (kind, &r) {
            s.inter.shift = onesim_render::autocap(text);
        }
        r
    });
    match e {
        Ok(e) => dispatch(e),
        Err(_) => -1,
    }
}

/// Replace the pasteboard with the JSON array of strings in the scratch buffer (`len` bytes). Returns 1 if the
/// guest took it (it re-renders if that changes anything), 0 for bad JSON or a guest without a pasteboard.
#[no_mangle]
pub extern "C" fn simcore_web_set_pasteboard(len: usize) -> i32 {
    let items: Option<Vec<String>> = with(|s| serde_json::from_slice(&s.buf[..len.min(s.buf.len())]).ok());
    match items {
        Some(items) if crate::state::send_pasteboard(&items) == 0 => 1,
        _ => 0,
    }
}

/// The items the app put on the pasteboard since the last call, as a JSON array in the scratch buffer.
/// Returns its length, or 0 when nothing changed.
#[no_mangle]
pub extern "C" fn simcore_web_take_pasteboard() -> usize {
    with(|s| match s.pasteboard_out.take() {
        Some(items) => {
            s.buf = serde_json::to_vec(&items).unwrap_or_default();
            s.buf.len()
        }
        None => 0,
    })
}

/// 1 while a text field is focused (the software keyboard is showing).
#[no_mangle]
pub extern "C" fn simcore_web_focused() -> i32 {
    with(|s| s.inter.focus.is_some() as i32)
}

/// Scratch buffer shared with JS: resized to `len`, returns its address.
#[no_mangle]
pub extern "C" fn simcore_web_buf(len: usize) -> *mut u8 {
    with(|s| {
        s.buf.resize(len, 0);
        s.buf.as_mut_ptr()
    })
}

#[no_mangle]
pub extern "C" fn simcore_web_buf_len() -> usize {
    with(|s| s.buf.len())
}

/// Inspection tree (protocol `UiNode`, as `inspect_ui_tree`) as JSON in the scratch buffer.
#[no_mangle]
pub extern "C" fn simcore_web_inspect() -> i32 {
    with(|s| {
        let Some(sc) = &s.scene else { return 1 };
        s.buf = serde_json::to_vec(&sc.ui_tree()).unwrap_or_default();
        0
    })
}

/// Resolve the label in the scratch buffer (`onesim_render::resolve_label` syntax, the CLI's
/// `--click LABEL`). 0: two little-endian f64 (x, y in points) are in the buffer; 1: error text.
#[no_mangle]
pub extern "C" fn simcore_web_find(len: usize) -> i32 {
    with(|s| {
        let label = String::from_utf8_lossy(&s.buf[..len.min(s.buf.len())]).into_owned();
        let found = match &s.scene {
            Some(sc) => resolve_label(&sc.ui_tree(), &label),
            None => Err("no scene".to_string()),
        };
        match found {
            Ok((x, y)) => {
                s.buf = [x.to_le_bytes(), y.to_le_bytes()].concat();
                0
            }
            Err(m) => {
                s.buf = m.into_bytes();
                1
            }
        }
    })
}

/// An image or font file the guest wrote (React Native `Image` sources and expo-font / expo-asset downloads are cached
/// as files): the scratch buffer holds the path, a NUL byte, then the file's bytes (`len` in all). Redraws so it shows.
/// 1 if it decoded as an image (PNG, JPEG, GIF, WebP) or a font, else 0.
#[no_mangle]
pub extern "C" fn simcore_web_put_image(len: usize) -> i32 {
    let ok = with(|s| {
        let b = &s.buf[..len.min(s.buf.len())];
        let Some(nul) = b.iter().position(|&c| c == 0) else { return false };
        let path = String::from_utf8_lossy(&b[..nul]);
        onesim_render::put_image(&path, &b[nul + 1..]) || onesim_render::put_font(&path, &b[nul + 1..])
    });
    if ok {
        with(update_scene);
    }
    i32::from(ok)
}
