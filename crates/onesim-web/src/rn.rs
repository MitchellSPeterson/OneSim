use crate::state::{dispatch, update_scene, with, Rn, State};
use onesim_protocol::{GuestMessage, PointerEvent};
use onesim_render::rn_input::{Out, RnCtx, RnInput};
use onesim_render::{device, look, Effect, Interaction, DEFAULT_DEVICE};

/// React Native mode: deliver `out` (messages to the outbox, native-control effects through `dispatch`) and update the scene.
pub(crate) fn deliver(out: Vec<Out>) -> i32 {
    let any = !out.is_empty();
    for o in out {
        match o {
            Out::Send(m, _) => with(|s| s.rn.as_mut().map_or((), |rn| rn.outbox.push(m))),
            Out::Effect(e) => drop(dispatch(e)),
        }
    }
    with(update_scene); // scroll offsets, focus and the keyboard are host state
    i32::from(any)
}

/// React Native mode: deliver coalesced drag output without rebuilding the scene on Redraw.
pub(crate) fn deliver_coalesced(out: Vec<Out>) -> i32 {
    let any = !out.is_empty();
    for o in out {
        match o {
            Out::Send(m, _) => with(|s| s.rn.as_mut().map_or((), |rn| rn.outbox.push(m))),
            Out::Effect(Effect::None | Effect::Redraw) => {}
            Out::Effect(e) => drop(dispatch(e)),
        }
    }
    i32::from(any)
}

/// Run `f` against the React Native input state, the interaction and the current scene and tree.
/// `None` outside React Native mode or before the first tree.
pub(crate) fn with_rn<R>(f: impl FnOnce(&mut RnInput, &mut Interaction, &RnCtx) -> R) -> Option<R> {
    with(|s| {
        let rn = s.rn.as_mut()?;
        let (scene, root) = (s.scene.as_ref()?, s.tree.as_ref()?);
        let cx = RnCtx { scene, root, keyboard: look::keyboard_height(s.device, s.ios) };
        Some(f(&mut rn.input, &mut s.inter, &cx))
    })
}

pub(crate) fn rn_pointer(ev: PointerEvent) -> Option<i32> {
    if with(|s| s.rn.is_none()) {
        return None;
    }
    let out = with_rn(|rn, inter, cx| rn.pointer(inter, cx, ev).2)?;
    Some(deliver(out))
}

/// React Native mode: queue the `scroll` events for scrollers a tick moved (before the scene is rebuilt).
pub(crate) fn queue_scroll_events(s: &mut State) {
    if let (Some(rn), Some(sc)) = (s.rn.as_mut(), s.scene.as_ref()) {
        for o in rn.input.scrolled(&s.inter, sc) {
            if let Out::Send(m, _) = o {
                rn.outbox.push(m);
            }
        }
    }
}

/// Switch to React Native mode and set the screen: `width` x `height` points, `scale`, `dark`, `android` (1 = the
/// default Android device unless `simcore_web_set_device` chose one). Nothing draws until the guest's first tree
/// (`simcore_web_rn_receive`). Call once instead of `simcore_web_init`.
#[no_mangle]
pub extern "C" fn simcore_web_rn_init(width: f64, height: f64, scale: f64, dark: i32, android: i32) -> i32 {
    with(|s| {
        s.viewport = (width.max(1.0), height.max(1.0));
        s.scale = scale.clamp(1.0, 4.0);
        s.dark = dark != 0;
        if (android != 0) != s.device.android {
            s.device = device(if android != 0 {
                onesim_render::DEFAULT_ANDROID_DEVICE
            } else {
                DEFAULT_DEVICE
            })
            .unwrap();
        }
        s.rn = Some(Rn::default());
    });
    0
}

/// One guest message (docs/protocol.md section H, JSON, `len` bytes in the scratch buffer). A `tree` updates the scene;
/// `focus_request` / `scroll_to` are applied. Returns 1 if the frame changed, 0 if not (other messages are the
/// page's), -1 on a bad message or a stale tree version (the reason is in the scratch buffer).
#[no_mangle]
pub extern "C" fn simcore_web_rn_receive(len: usize) -> i32 {
    let msg = with(|s| serde_json::from_slice::<GuestMessage>(&s.buf[..len.min(s.buf.len())]));
    let fail = |m: String| {
        with(|s| s.buf = m.into_bytes());
        -1
    };
    match msg {
        Err(e) => fail(format!("bad guest message: {e}")),
        Ok(GuestMessage::Tree { version, root }) => {
            let stale = with(|s| s.version.is_some_and(|v| version <= v));
            if stale {
                return fail("tree version must increase monotonically".into());
            }
            with(|s| {
                s.version = Some(version);
                s.inter.version = version;
                s.tree_json.clear();
                s.layout_cache.clear();
                s.tree = Some(*root);
                update_scene(s);
            });
            rn_requests(vec![]); // drop the focus of a field that is gone
            1
        }
        Ok(
            m @ (GuestMessage::FocusRequest { .. }
            | GuestMessage::ScrollTo { .. }
            | GuestMessage::JsResponder { .. }
            | GuestMessage::TouchAnswer { .. }),
        ) => rn_requests(vec![m]),
        // A new guest instance (first start or reload): it knows no frames yet and has no touch in progress.
        Ok(GuestMessage::Hello { .. }) => with(|s| {
            if let Some(rn) = s.rn.as_mut() {
                rn.input = RnInput::default();
            }
            0
        }),
        // Image hashes and text rendering for the guest (protocol section H): answered at once through the outbox.
        Ok(GuestMessage::Request { id, method, params }) => with(|s| {
            let fonts = s.fonts.get_or_insert_with(|| onesim_render::Fonts::with_fallback(true)); // browsers: bundled Inter only
            let (result, error) = match onesim_render::guest_request::handle(fonts, &method, &params) {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e)),
            };
            if let Some(rn) = s.rn.as_mut() {
                rn.outbox.push(onesim_protocol::HostMessage::Response { id, result, error });
            }
            0
        }),
        Ok(_) => 0,
    }
}

fn rn_requests(requests: Vec<GuestMessage>) -> i32 {
    match with_rn(|rn, inter, cx| rn.apply_requests(inter, cx, requests)) {
        Some((true, out)) => {
            deliver(out);
            1
        }
        Some((false, out)) => {
            if out.is_empty() {
                0
            } else {
                deliver(out)
            }
        }
        None => 0,
    }
}

/// Move the host messages waiting for the guest into the scratch buffer as a JSON array; returns how many.
/// The page posts each one to the guest Worker.
#[no_mangle]
pub extern "C" fn simcore_web_rn_outbox() -> i32 {
    with(|s| {
        let out = s.rn.as_mut().map(|rn| std::mem::take(&mut rn.outbox)).unwrap_or_default();
        s.buf = serde_json::to_vec(&out).unwrap_or_default();
        out.len() as i32
    })
}
