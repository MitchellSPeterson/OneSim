#!/usr/bin/env bash
# Usage: serve-web.sh [--no-build] [port]
# Builds every SwiftUI, Jetpack Compose, and Expo / React Native example into target/web-all/,
# writes an index page split by platform (iOS & Android), and serves it on http://localhost:<port>/ (default 8000).
# Pass --no-build to refresh the web shell and index page using existing Wasm and Expo bundles.
# Needs the toolchain in docs/web.md.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NO_BUILD=0
if [ "${1:-}" = "--no-build" ]; then NO_BUILD=1; shift; fi
PORT="${1:-8000}"
SITE="${CARGO_TARGET_DIR:-$ROOT/target}/web-all"

# Native Wasm builds (SwiftUI & Compose)
# example|platform|root_view|title|tag|description  (root_view '-' = @main / ComponentActivity entry point)
NATIVE_BUILDS="
swiftui-showcase|ios|-|Showcase|Featured|Multi-screen tour of SwiftUI controls, typography, todos, alerts, sheets, and Liquid Glass.
swiftui-counter|ios|CounterView|Counter||Minimal stateful counter demonstrating @State updates and button actions.
swiftui-app-entry|ios|-|App Entry||Custom @main App entry point with reusable interactive badge subviews.
swiftui-dataflow|ios|-|Dataflow||@EnvironmentObject, @AppStorage, onChange, lazy stacks, and value-based navigation.
swiftui-gestures|ios|-|Gestures||Interactive drag with @GestureState, long press, double tap, and pinch-rotate.
swiftui-presentation|ios|-|Presentation||Sheets with detents, locked modals, full-screen covers, alerts, and confirmation dialogs.
swiftui-tabs|ios|-|Tabs||Three-tab TabView with nested navigation stack, preserved state, and badges.
compose-showcase|android|-|Showcase|Featured|Material 3 components, cards, switches, buttons, and adaptive light and dark theming.
compose-counter|android|-|Counter||Edge-to-edge Material 3 counter built with ComponentActivity and Compose state.
compose-input|android|-|Input||Interactive text fields, software keyboard input, and Material 3 sliders.
"

# Expo / React Native builds (exported per platform, served via shared target/web-all/expo/)
# example|platforms|title|tag|ios_desc|android_desc
EXPO_BUILDS="
expo-showcase|ios,android|Showcase|Featured|Multi-screen React Native tour of layout, typography, controls, scroll, images, and gradients.|Multi-screen React Native tour of layout, typography, controls, scroll, images, and gradients.
expo-ui|ios,android|Expo UI||Native SwiftUI components from @expo/ui hosted seamlessly inside React Native views.|Native Jetpack Compose components from @expo/ui hosted seamlessly inside React Native views.
expo-router-stack|ios,android|Router Stack||Expo Router native stack navigation with large titles and dynamic detail routes.|Expo Router stack navigation with header transitions and dynamic detail routes.
expo-default|ios|Starter Template||Default Expo Router template with Liquid Glass tabs, themed surfaces, and interactive assets.|
expo-counter|ios,android|Counter||Minimal stateful React Native counter built with Pressable and useState.|Minimal stateful React Native counter built with Pressable and useState.
expo-gestures|ios,android|Gestures||react-native-gesture-handler and Reanimated tap, pan, long press, and swipeable rows.|react-native-gesture-handler and Reanimated tap, pan, long press, and swipeable rows.
expo-modules|ios,android|SDK Modules||Expo SDK modules demo: Clipboard, FileSystem, SecureStore, AsyncStorage, and Crypto.|Expo SDK modules demo: Clipboard, FileSystem, SecureStore, AsyncStorage, and Crypto.
"

mkdir -p "$SITE" "$SITE/assets"
IOS=""; ANDROID=""
IOS_COUNT=0; ANDROID_COUNT=0
ARROW_SVG='<svg class="card-arrow" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4.5 11.5 11.5 4.5M6 4.5h5.5V10"/></svg>'

patch_index() {
  python3 - "$@" <<'PY'
import html, sys
path, index_href = sys.argv[1], sys.argv[2]
title = sys.argv[3] if len(sys.argv) > 3 else ""
fw = sys.argv[4] if len(sys.argv) > 4 else ""
s = open(path, encoding="utf-8").read()
attrs = f'data-index="{html.escape(index_href, quote=True)}"'
if title:
    t_esc = html.escape(title, quote=True)
    s = s.replace("<title>OneSim Web</title>", f"<title>{t_esc} — OneSim</title>", 1)
    attrs += f' data-title="{t_esc}"'
if fw:
    attrs += f' data-framework="{html.escape(fw, quote=True)}"'
s = s.replace("<body>", f"<body {attrs}>", 1)
open(path, "w", encoding="utf-8").write(s)
PY
}

# 1. SwiftUI & Jetpack Compose builds
while IFS='|' read -r ex platform view title tag desc; do
  [ -n "$ex" ] || continue
  if [ "$NO_BUILD" -eq 1 ] && [ -f "$SITE/$ex/app.wasm" ]; then
    cp "$ROOT/web/index.html" "$ROOT/web/main.js" "$ROOT/web/onesim.js" "$ROOT/web/renderer.js" "$SITE/$ex/"
  else
    echo "== $ex"
    [ "$view" = - ] && view=""
    SIMCORE_WEB_OUT="$SITE/$ex" "$ROOT/scripts/build-web.sh" "$ROOT/examples/$ex" $view
  fi

  fw="${ex%%-*}"
  if [ "$fw" = "compose" ]; then fw_label="Compose"; else fw_label="SwiftUI"; fi
  patch_index "$SITE/$ex/index.html" "../" "$title" "$fw_label"

  badge_html=""
  if [ -n "$tag" ]; then badge_html="<span class=\"badge-featured\">$tag</span>"; fi
  card="<a class=\"card\" href=\"$ex/\" data-fw=\"$fw\"><div class=\"card-top\"><div class=\"card-badges\"><span class=\"badge-fw\">$fw_label</span>$badge_html</div>$ARROW_SVG</div><h3 class=\"card-title\">$title</h3><p class=\"card-desc\">$desc</p></a>"
  if [ "$platform" = android ]; then
    ANDROID="$ANDROID$card"
    ANDROID_COUNT=$((ANDROID_COUNT + 1))
  else
    IOS="$IOS$card"
    IOS_COUNT=$((IOS_COUNT + 1))
  fi
done <<< "$NATIVE_BUILDS"

# 2. Shared Expo / React Native web runtime + exported bundles for iOS and Android
if [ "$NO_BUILD" -eq 1 ] && [ -f "$SITE/expo/app.wasm" ]; then
  cp "$ROOT/web/index.html" "$ROOT/web/main.js" "$ROOT/web/onesim.js" "$ROOT/web/renderer.js" "$SITE/expo/"
  cp "$ROOT"/frameworks/react-native/guest/*.js "$SITE/expo/guest/"
  cp "$ROOT"/web/expo/*.js "$SITE/expo/expo/"
else
  echo "== expo runtime"
  SIMCORE_WEB_OUT="$SITE/expo" "$ROOT/scripts/build-web.sh" --expo
fi
patch_index "$SITE/expo/index.html" "../"

while IFS='|' read -r ex platforms title tag ios_desc android_desc; do
  [ -n "$ex" ] || continue
  bundle_dir="$SITE/expo/bundles/$ex"
  if [ "$NO_BUILD" -eq 0 ] || [ ! -f "$bundle_dir/metadata.json" ]; then
    echo "== $ex ($platforms)"
    if [ ! -d "$ROOT/examples/$ex/node_modules" ]; then
      npm ci --prefix "$ROOT/examples/$ex" --prefer-offline --no-audit --no-fund --loglevel=error
    fi
    plat_args=()
    [[ ",$platforms," == *,ios,* ]] && plat_args+=(--platform ios)
    [[ ",$platforms," == *,android,* ]] && plat_args+=(--platform android)
    rm -rf "$bundle_dir"
    (cd "$ROOT/examples/$ex" && CI=1 EXPO_OFFLINE=1 EXPO_NO_TELEMETRY=1 npx expo export "${plat_args[@]}" --no-bytecode --no-minify --output-dir "$bundle_dir" >/dev/null)
  fi
  if [ -d "$bundle_dir/assets" ]; then
    cp -f "$bundle_dir/assets/"* "$SITE/assets/" 2>/dev/null || true
  fi

  read -r ios_bundle android_bundle < <(python3 - "$bundle_dir/metadata.json" <<'PY'
import json, sys
meta = json.load(open(sys.argv[1], encoding="utf-8"))["fileMetadata"]
print(meta.get("ios", {}).get("bundle", "-"), meta.get("android", {}).get("bundle", "-"))
PY
)
  title_q="$(python3 -c 'import urllib.parse, sys; print(urllib.parse.quote(sys.argv[1]))' "$title")"
  badge_html=""
  if [ -n "$tag" ]; then badge_html="<span class=\"badge-featured\">$tag</span>"; fi

  if [[ ",$platforms," == *,ios,* ]] && [ "$ios_bundle" != "-" ]; then
    ios_href="expo/?bundle=/expo/bundles/$ex/$ios_bundle&amp;platform=ios&amp;title=$title_q&amp;framework=Expo"
    IOS="$IOS<a class=\"card\" href=\"$ios_href\" data-fw=\"expo\"><div class=\"card-top\"><div class=\"card-badges\"><span class=\"badge-fw\">Expo</span>$badge_html</div>$ARROW_SVG</div><h3 class=\"card-title\">$title</h3><p class=\"card-desc\">$ios_desc</p></a>"
    IOS_COUNT=$((IOS_COUNT + 1))
  fi
  if [[ ",$platforms," == *,android,* ]] && [ "$android_bundle" != "-" ]; then
    android_href="expo/?bundle=/expo/bundles/$ex/$android_bundle&amp;platform=android&amp;title=$title_q&amp;framework=Expo"
    ANDROID="$ANDROID<a class=\"card\" href=\"$android_href\" data-fw=\"expo\"><div class=\"card-top\"><div class=\"card-badges\"><span class=\"badge-fw\">Expo</span>$badge_html</div>$ARROW_SVG</div><h3 class=\"card-title\">$title</h3><p class=\"card-desc\">$android_desc</p></a>"
    ANDROID_COUNT=$((ANDROID_COUNT + 1))
  fi
done <<< "$EXPO_BUILDS"

TOTAL_COUNT=$((IOS_COUNT + ANDROID_COUNT))

cat > "$SITE/index.html" <<HTML
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OneSim — Web Builds</title>
<script>
(() => {
  let theme = null;
  try { const s = localStorage.getItem("onesim.theme"); if (s === "dark" || s === "light") theme = s; } catch {}
  if (!theme) theme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
})();
</script>
<style>
  :root {
    --bg: #f6f7f9;
    --bg-spot: #ffffff;
    --dot: rgba(15, 23, 42, 0.045);
    --nav: rgba(246, 247, 249, 0.8);
    --surface: #ffffff;
    --surface-sub: #f0f2f5;
    --border: rgba(15, 23, 42, 0.085);
    --border-hover: rgba(15, 23, 42, 0.18);
    --ink: #0f1218;
    --ink-sub: #576071;
    --ink-muted: #8690a0;
    --accent: #0066ff;
    --ios: #0071e3;
    --ios-bg: rgba(0, 113, 227, 0.09);
    --android: #059669;
    --android-bg: rgba(16, 185, 129, 0.1);
    --expo: #7c3aed;
    --expo-bg: rgba(124, 58, 237, 0.1);
    --featured: #4f46e5;
    --featured-bg: rgba(99, 102, 241, 0.1);
    --shadow-card: 0 1px 2px rgba(15, 23, 42, 0.04), 0 4px 12px -4px rgba(15, 23, 42, 0.05);
    --shadow-card-hover: 0 2px 4px rgba(15, 23, 42, 0.05), 0 14px 30px -10px rgba(15, 23, 42, 0.14);
    color-scheme: light;
  }

  :root[data-theme="dark"] {
    --bg: #08090c;
    --bg-spot: #151924;
    --dot: rgba(255, 255, 255, 0.04);
    --nav: rgba(8, 9, 12, 0.78);
    --surface: #111319;
    --surface-sub: #1a1d26;
    --border: rgba(255, 255, 255, 0.085);
    --border-hover: rgba(255, 255, 255, 0.2);
    --ink: #f3f4f6;
    --ink-sub: #9ba3b2;
    --ink-muted: #676f7e;
    --accent: #3b82f6;
    --ios: #58a6ff;
    --ios-bg: rgba(59, 130, 246, 0.14);
    --android: #34d399;
    --android-bg: rgba(52, 211, 153, 0.13);
    --expo: #a78bfa;
    --expo-bg: rgba(167, 139, 250, 0.15);
    --featured: #a5b4fc;
    --featured-bg: rgba(129, 140, 248, 0.14);
    --shadow-card: 0 1px 0 rgba(255, 255, 255, 0.04) inset, 0 4px 16px -6px rgba(0, 0, 0, 0.5);
    --shadow-card-hover: 0 1px 0 rgba(255, 255, 255, 0.07) inset, 0 18px 36px -12px rgba(0, 0, 0, 0.75);
    color-scheme: dark;
  }

  * { box-sizing: border-box; }
  html { scroll-behavior: smooth; }
  body {
    margin: 0;
    min-height: 100vh;
    font: 15px/1.5 -apple-system, BlinkMacSystemFont, "SF Pro Text", "Inter", system-ui, sans-serif;
    color: var(--ink);
    background-color: var(--bg);
    background-image:
      radial-gradient(circle at 1px 1px, var(--dot) 1px, transparent 0),
      radial-gradient(ellipse 85% 45% at 50% 0%, var(--bg-spot), transparent 70%);
    background-size: 24px 24px, 100% 100%;
    background-attachment: fixed;
    -webkit-font-smoothing: antialiased;
    transition: background-color 0.2s ease, color 0.2s ease;
  }

  /* Sticky frosted navigation */
  .nav {
    position: sticky;
    top: 0;
    z-index: 20;
    background: var(--nav);
    backdrop-filter: blur(18px) saturate(180%);
    -webkit-backdrop-filter: blur(18px) saturate(180%);
    border-bottom: 1px solid var(--border);
  }
  .nav-inner {
    max-width: 1120px;
    margin: 0 auto;
    padding: 0 24px;
    height: 58px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    color: var(--ink);
    text-decoration: none;
    font-weight: 650;
    font-size: 15.5px;
    letter-spacing: -0.02em;
  }
  .brand-mark {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    background: linear-gradient(145deg, #1d2433, #0b0e14);
    color: #fff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.16);
  }
  :root[data-theme="dark"] .brand-mark {
    background: linear-gradient(145deg, #273046, #141822);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }
  .brand-mark svg { width: 16px; height: 16px; }
  .brand-tag {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--ink-sub);
    background: var(--surface-sub);
    border: 1px solid var(--border);
    padding: 2px 7px;
    border-radius: 999px;
  }

  .nav-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .segmented {
    display: inline-flex;
    align-items: center;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-sub);
    border: 1px solid var(--border);
    gap: 2px;
  }
  .segmented.compact { padding: 2px; }
  .seg-btn {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--ink-sub);
    font: inherit;
    font-size: 12.5px;
    font-weight: 500;
    height: 28px;
    padding: 0 12px;
    border-radius: 999px;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease, box-shadow 0.15s ease;
  }
  .segmented.compact .seg-btn {
    height: 24px;
    padding: 0 10px;
    font-size: 11.5px;
  }
  .seg-btn:hover { color: var(--ink); }
  .seg-btn[aria-pressed="true"] {
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 1px 2px rgba(15, 23, 42, 0.08);
  }
  :root[data-theme="dark"] .seg-btn[aria-pressed="true"] {
    background: #222634;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.45);
  }
  .theme-btn {
    appearance: none;
    width: 34px;
    height: 34px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--ink-sub);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: border-color 0.15s ease, color 0.15s ease, background 0.15s ease;
  }
  .theme-btn:hover, .theme-btn:focus-visible {
    border-color: var(--border-hover);
    color: var(--ink);
    outline: none;
  }
  .theme-btn svg { width: 15px; height: 15px; }
  :root[data-theme="dark"] .ico-moon { display: none; }
  :root:not([data-theme="dark"]) .ico-sun { display: none; }

  /* Main content */
  main {
    max-width: 1120px;
    margin: 0 auto;
    padding: 38px 24px 56px;
  }

  .hero {
    max-width: 680px;
    margin-bottom: 36px;
  }
  .eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 11px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--ink-sub);
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 0.01em;
    margin-bottom: 14px;
    box-shadow: var(--shadow-card);
  }
  .eyebrow-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #10b981;
    box-shadow: 0 0 0 3px rgba(16, 185, 129, 0.2);
  }
  h1 {
    margin: 0 0 10px;
    font-size: clamp(30px, 4vw, 40px);
    font-weight: 700;
    line-height: 1.08;
    letter-spacing: -0.035em;
  }
  .hero-sub {
    margin: 0;
    font-size: 15.5px;
    line-height: 1.52;
    color: var(--ink-sub);
  }

  /* Platform sections */
  .platform {
    margin-bottom: 40px;
  }
  .platform:last-of-type { margin-bottom: 0; }
  .platform[hidden] { display: none; }

  .section-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 16px;
    padding-bottom: 11px;
    border-bottom: 1px solid var(--border);
  }
  .section-title-group {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .section-tools {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .platform-icon {
    width: 34px;
    height: 34px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    flex: none;
  }
  .platform-icon.ios { background: var(--ios-bg); color: var(--ios); }
  .platform-icon.android { background: var(--android-bg); color: var(--android); }
  .platform-icon svg { width: 17px; height: 17px; }
  .section-head h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1.2;
  }
  .section-meta {
    display: block;
    font-size: 12.5px;
    color: var(--ink-muted);
    margin-top: 1px;
  }
  .section-count {
    font-size: 12px;
    font-weight: 500;
    color: var(--ink-sub);
    background: var(--surface-sub);
    border: 1px solid var(--border);
    padding: 3px 10px;
    border-radius: 999px;
    white-space: nowrap;
  }

  /* Cards grid */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(242px, 1fr));
    gap: 14px;
  }
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    padding: 18px 19px 19px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-card);
    color: inherit;
    text-decoration: none;
    transition: transform 0.18s cubic-bezier(0.2, 0.8, 0.2, 1), border-color 0.18s ease, box-shadow 0.18s ease;
  }
  .card[hidden] { display: none; }
  .card:hover, .card:focus-visible {
    transform: translateY(-2px);
    border-color: var(--border-hover);
    box-shadow: var(--shadow-card-hover);
    outline: none;
  }
  .card[data-fw="swiftui"]:hover, .card[data-fw="swiftui"]:focus-visible {
    border-color: rgba(0, 113, 227, 0.42);
  }
  .card[data-fw="compose"]:hover, .card[data-fw="compose"]:focus-visible {
    border-color: rgba(16, 185, 129, 0.42);
  }
  .card[data-fw="expo"]:hover, .card[data-fw="expo"]:focus-visible {
    border-color: rgba(124, 58, 237, 0.44);
  }

  .card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 14px;
  }
  .card-badges {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .badge-fw {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    padding: 3px 8px;
    border-radius: 6px;
  }
  .card[data-fw="swiftui"] .badge-fw { background: var(--ios-bg); color: var(--ios); }
  .card[data-fw="compose"] .badge-fw { background: var(--android-bg); color: var(--android); }
  .card[data-fw="expo"] .badge-fw { background: var(--expo-bg); color: var(--expo); }
  .badge-featured {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.01em;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--featured-bg);
    color: var(--featured);
  }
  .card-arrow {
    width: 16px;
    height: 16px;
    color: var(--ink-muted);
    transition: transform 0.18s cubic-bezier(0.2, 0.8, 0.2, 1), color 0.18s ease;
  }
  .card:hover .card-arrow, .card:focus-visible .card-arrow {
    transform: translate(2px, -2px);
    color: var(--ink);
  }
  .card[data-fw="swiftui"]:hover .card-arrow { color: var(--ios); }
  .card[data-fw="compose"]:hover .card-arrow { color: var(--android); }
  .card[data-fw="expo"]:hover .card-arrow { color: var(--expo); }

  .card-title {
    margin: 0 0 6px;
    font-size: 17px;
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1.25;
  }
  .card-desc {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.5;
    color: var(--ink-sub);
  }

  /* Footer */
  footer {
    max-width: 1120px;
    margin: 0 auto;
    padding: 18px 24px 36px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 12.5px;
    color: var(--ink-muted);
  }
  footer code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    padding: 2px 5px;
    border-radius: 5px;
    background: var(--surface-sub);
    color: var(--ink-sub);
    border: 1px solid var(--border);
  }

  @media (max-width: 640px) {
    .nav-inner { padding: 0 16px; }
    main { padding: 32px 16px 52px; }
    footer { padding: 18px 16px 32px; }
    .brand-tag { display: none; }
    .seg-btn { padding: 0 9px; font-size: 12px; }
    .grid { grid-template-columns: 1fr; }
  }
</style>
</head>
<body>
<nav class="nav" aria-label="Primary">
  <div class="nav-inner">
    <a class="brand" href="./">
      <span class="brand-mark" aria-hidden="true">
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="2.25" width="10" height="15.5" rx="2.6"/><path d="M8.5 5h3M8.8 14.8h2.4"/></svg>
      </span>
      <span>OneSim</span>
      <span class="brand-tag">Web</span>
    </a>
    <div class="nav-actions">
      <div class="segmented" role="group" aria-label="Filter by platform">
        <button class="seg-btn" type="button" data-filter="all" aria-pressed="true">All ($TOTAL_COUNT)</button>
        <button class="seg-btn" type="button" data-filter="ios" aria-pressed="false">iOS</button>
        <button class="seg-btn" type="button" data-filter="android" aria-pressed="false">Android</button>
      </div>
      <button id="theme-btn" class="theme-btn" type="button" title="Toggle appearance" aria-label="Toggle appearance">
        <svg class="ico-sun" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><circle cx="8" cy="8" r="3"/><path d="M8 1.5v1.4M8 13.1v1.4M1.5 8h1.4M13.1 8h1.4M3.4 3.4l1 1M11.6 11.6l1 1M12.6 3.4l-1 1M4.4 11.6l-1 1"/></svg>
        <svg class="ico-moon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13.2 9.8A5.5 5.5 0 0 1 6.2 2.8a5.5 5.5 0 1 0 7 7Z"/></svg>
      </button>
    </div>
  </div>
</nav>

<main>
  <header class="hero">
    <div class="eyebrow"><span class="eyebrow-dot" aria-hidden="true"></span><span>Client-side WebAssembly · WebGPU</span></div>
    <h1>OneSim Web Builds</h1>
    <p class="hero-sub">Interactive iOS and Android apps running locally in your browser—SwiftUI, Jetpack Compose, and Expo / React Native powered by the OneSim WebAssembly renderer.</p>
  </header>

  <section class="platform" data-platform="ios" aria-labelledby="heading-ios">
    <div class="section-head">
      <div class="section-title-group">
        <div class="platform-icon ios" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="2.2" width="10" height="15.6" rx="2.5"/><path d="M8.5 4.8h3M9 15.2h2"/></svg>
        </div>
        <div>
          <h2 id="heading-ios">iOS</h2>
          <span class="section-meta">SwiftUI &amp; Expo · iPhone 18 Pro</span>
        </div>
      </div>
      <div class="section-tools">
        <div class="segmented compact" role="group" aria-label="Filter iOS frameworks">
          <button class="seg-btn" type="button" data-fw-filter="all" aria-pressed="true">All</button>
          <button class="seg-btn" type="button" data-fw-filter="swiftui" aria-pressed="false">SwiftUI</button>
          <button class="seg-btn" type="button" data-fw-filter="expo" aria-pressed="false">Expo</button>
        </div>
        <span class="section-count" data-count>$IOS_COUNT builds</span>
      </div>
    </div>
    <div class="grid">$IOS</div>
  </section>

  <section class="platform" data-platform="android" aria-labelledby="heading-android">
    <div class="section-head">
      <div class="section-title-group">
        <div class="platform-icon android" aria-hidden="true">
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 11.5a5.5 5.5 0 0 1 11 0v1.5a1.5 1.5 0 0 1-1.5 1.5H6A1.5 1.5 0 0 1 4.5 13v-1.5Z"/><path d="m7 6.5-1.3-2.1M13 6.5l1.3-2.1"/><circle cx="7.8" cy="10.8" r=".7" fill="currentColor" stroke="none"/><circle cx="12.2" cy="10.8" r=".7" fill="currentColor" stroke="none"/></svg>
        </div>
        <div>
          <h2 id="heading-android">Android</h2>
          <span class="section-meta">Jetpack Compose &amp; Expo · Pixel 10a</span>
        </div>
      </div>
      <div class="section-tools">
        <div class="segmented compact" role="group" aria-label="Filter Android frameworks">
          <button class="seg-btn" type="button" data-fw-filter="all" aria-pressed="true">All</button>
          <button class="seg-btn" type="button" data-fw-filter="compose" aria-pressed="false">Compose</button>
          <button class="seg-btn" type="button" data-fw-filter="expo" aria-pressed="false">Expo</button>
        </div>
        <span class="section-count" data-count>$ANDROID_COUNT builds</span>
      </div>
    </div>
    <div class="grid">$ANDROID</div>
  </section>
</main>

<footer>
  <span>OneSim · Local WebAssembly dev server</span>
  <span>Override any build with <code>?device=</code>, <code>?dark=1</code>, or <code>?renderer=cpu</code></span>
</footer>

<script>
(() => {
  const root = document.documentElement;
  const themeBtn = document.getElementById("theme-btn");
  const syncThemeLabel = () => {
    const next = root.dataset.theme === "dark" ? "Switch to light mode" : "Switch to dark mode";
    themeBtn.title = next;
    themeBtn.setAttribute("aria-label", next);
  };
  syncThemeLabel();
  themeBtn.addEventListener("click", () => {
    const next = root.dataset.theme === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    try { localStorage.setItem("onesim.theme", next); } catch {}
    syncThemeLabel();
  });

  const platformBtns = [...document.querySelectorAll("[data-filter]")];
  const sections = [...document.querySelectorAll(".platform")];
  platformBtns.forEach((btn) => btn.addEventListener("click", () => {
    const f = btn.dataset.filter;
    platformBtns.forEach((b) => b.setAttribute("aria-pressed", String(b === btn)));
    sections.forEach((s) => { s.hidden = f !== "all" && s.dataset.platform !== f; });
  }));

  sections.forEach((sec) => {
    const fwBtns = [...sec.querySelectorAll("[data-fw-filter]")];
    const cards = [...sec.querySelectorAll(".card")];
    const countEl = sec.querySelector("[data-count]");
    fwBtns.forEach((btn) => btn.addEventListener("click", () => {
      const fw = btn.dataset.fwFilter;
      fwBtns.forEach((b) => b.setAttribute("aria-pressed", String(b === btn)));
      let visible = 0;
      cards.forEach((c) => {
        const show = fw === "all" || c.dataset.fw === fw;
        c.hidden = !show;
        if (show) visible++;
      });
      if (countEl) countEl.textContent = visible + (visible === 1 ? " build" : " builds");
    }));
  });
})();
</script>
</body>
</html>
HTML

echo "serving $SITE on http://localhost:$PORT/"
exec python3 -m http.server "$PORT" -b 127.0.0.1 -d "$SITE"
