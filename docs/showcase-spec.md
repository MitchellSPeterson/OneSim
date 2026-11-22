# Prism — OneSim Unified Showcase App Spec

**Prism** is the unified, playful showcase app for the OneSim web demo site (`scripts/serve-web.sh`). Instead of separate single-feature demos per platform, OneSim builds **one cohesive app** for each framework/platform target:

- **SwiftUI (iOS):** `examples/swiftui-prism`
- **Jetpack Compose (Android):** `examples/compose-prism`
- **Expo / React Native (iOS & Android):** `examples/expo-prism`

Every target shares the same app identity, color palettes, tab structure, and interactive personality, while showcasing **only the features currently supported on that platform**. As new OneSim capabilities land, update this spec's compatibility matrix and enable the corresponding feature in that platform's Prism app.

---

## 1. Visual Identity & Palettes

Prism is a colorful, tactile daily creative studio & mood playground. It adapts cleanly to both **Light** and **Dark** appearance and lets the user switch between 4 vibrant themes that recolor the hero, buttons, badges, and interactive elements live:

| Palette | Primary | Secondary | Accent / Glow | Mood Tag |
| --- | --- | --- | --- | --- |
| **Coral** | `#FF5E62` | `#FF9966` | `#FFD166` | Sunset Energy |
| **Mint** | `#00B894` | `#00CEC9` | `#55EFC4` | Fresh Focus |
| **Berry** | `#8E44AD` | `#E84393` | `#FD79A8` | Electric Flow |
| **Solar** | `#F39C12` | `#E67E22` | `#F1C40F` | Golden Spark |

---

## 2. Canonical App Structure

Prism has **3 primary tabs** (plus optional modal/detail flows where supported):

### Tab 1: **Pulse** (Home Studio)
- **Hero Vibe Card:** Colorful gradient (or vibrant M3 themed surface on Compose) showing the active palette name, current **Vibe Score** (interactive counter that increments on tap/boost), playful layered color orbs, and a live status pill (Liquid Glass / BlurView where supported).
- **Palette Switcher:** 4 interactive color pills (`Coral`, `Mint`, `Berry`, `Solar`) that re-theme the entire app immediately.
- **Interactive Playground:**
  - **Boost / Vibe Pad:** Tap to surge energy (with spring/bounce animation where supported).
  - **Draggable / Animated Orb:** Drag with spring snap-back (SwiftUI), animated floating/pulsing orbs (SwiftUI & Expo), or tactile Boost/Reset controls (Compose).
  - **Quick Controls:** Mood/Energy meter (`Gauge`/`ProgressView` or custom animated bar) + `Switch`/`Toggle` for **Glow Mode** (or **Zen Mode**).

### Tab 2: **Sparks** (Creative Micro-Quests)
- Colorful list of playful creative prompts (e.g., *"Sketch a cloud"*, *"Capture a sunset"*, *"Ship a tiny delight"*, *"Hydrate & stretch"*) with category tags (`Play`, `Make`, `Move`, `Calm`) and completion progress.
- **Toggle completion:** Tap any Spark card to mark it done/undone and watch the progress bar and completed count update live.
- **Create a Spark:** Add a custom Spark via text input + category/palette selection (`.sheet` on SwiftUI, `Modal` on Expo, inline `TextField` + FAB on Compose).
- **Actions:** Clear completed sparks with confirmation (`.alert` on SwiftUI & Expo iOS, modal confirm on Expo Android, button action on Compose); pull-to-refresh on Expo; swipe/detail push on SwiftUI.

### Tab 3: **Lab** (Interactive Controls & Effects)
- **Live Preview Card:** A colorful, real-time preview card whose shape, corner radius, intensity/opacity, and style respond immediately to controls below.
- **Controls:**
  - Sliders for corner radius & glow/rotation (SwiftUI, Compose) or interactive step pills (Expo).
  - Switches/Toggles for visual modes (Glass/Blur, Elevated/Outlined, Auto-Pulse).
  - Button style gallery (Filled, Tonal, Outlined, Text / Glass) with live feedback.
  - Segmented/mode picker (SwiftUI `Picker`, Expo/Compose pill selector).
- **Deep Dive / Focus Mode:**
  - **Focus Mode:** Full-screen colorful breathing orb presentation (`.fullScreenCover` on SwiftUI, full-screen `Modal` on Expo).
  - **Effects & Symbols Detail:** Pushed via `NavigationStack` on SwiftUI to showcase `AngularGradient`/`RadialGradient`, `MeshGradient`, Liquid Glass, and SF Symbols.

---

## 3. Feature Compatibility Matrix

Legend: `✅ Live` (implemented in Prism) · `🟡 Adapted` (using platform-available substitute) · `⬜ Pending` (not yet supported by OneSim on this target)

| Feature | SwiftUI (iOS) | Compose (Android) | Expo (iOS) | Expo (Android) |
| --- | :---: | :---: | :---: | :---: |
| Light & Dark appearance | ✅ Live | ✅ Live | ✅ Live | ✅ Live |
| Live 4-palette theme switcher | ✅ Live | ✅ Live | ✅ Live | ✅ Live |
| Multi-tab navigation (3 tabs) | ✅ Live (`TabView`) | 🟡 Adapted (`Scaffold.bottomBar`) | 🟡 Adapted (Custom tab bar) | 🟡 Adapted (Custom tab bar) |
| Stack push navigation | ✅ Live (`NavigationStack`) | ⬜ Pending | ⬜ Pending | ⬜ Pending |
| Gradients (`LinearGradient` / `MeshGradient`) | ✅ Live | ⬜ Pending (solid M3 colors) | ✅ Live (`expo-linear-gradient`) | ✅ Live (`expo-linear-gradient`) |
| Blur / Liquid Glass | ✅ Live (`.glassEffect`) | ⬜ Pending | ✅ Live (`BlurView`) | ✅ Live (`BlurView`) |
| Animations (spring / pulsing orbs) | ✅ Live (`withAnimation`, `PhaseAnimator`) | ⬜ Pending (no Compose animation/clock loop) | ✅ Live (`Animated`) | ✅ Live (`Animated`) |
| Drag / Long-press / Double-tap gestures | ✅ Live (`DragGesture`, etc.) | 🟡 Adapted (`clickable`) | 🟡 Adapted (`Pressable`) | 🟡 Adapted (`Pressable`) |
| Vertical scrolling & lists | ✅ Live (`ScrollView`, `List`) | ⬜ Pending (fixed-height layouts) | ✅ Live (`ScrollView`, `FlatList`) | ✅ Live (`ScrollView`, `FlatList`) |
| Pull-to-refresh | ⬜ Pending | ⬜ Pending | ✅ Live (`RefreshControl`) | ✅ Live (`RefreshControl`) |
| Text input & keyboard | ✅ Live (`TextField`) | ✅ Live (`TextField`) | ✅ Live (`TextInput`) | 🟡 Adapted (`TextInput`, no soft kbd) |
| Sliders | ✅ Live (`Slider`) | ✅ Live (`Slider`) | 🟡 Adapted (Preset pills) | 🟡 Adapted (Preset pills) |
| Switches / Toggles | ✅ Live (`Toggle`) | ✅ Live (`Switch`) | ✅ Live (`Switch`) | ✅ Live (`Switch`) |
| Steppers & Pickers | ✅ Live (`Stepper`, `Picker`) | 🟡 Adapted (Button row) | 🟡 Adapted (Pill row) | 🟡 Adapted (Pill row) |
| Progress / Gauge | ✅ Live (`Gauge`, `ProgressView`) | 🟡 Adapted (Custom bar) | 🟡 Adapted (Animated bar) | 🟡 Adapted (Animated bar) |
| Sheets / Modals / Full-Screen Cover | ✅ Live (`.sheet`, `.fullScreenCover`) | ⬜ Pending | ✅ Live (`Modal`) | ✅ Live (`Modal`) |
| Alerts / Confirmation Dialogs | ✅ Live (`.alert`, `.confirmationDialog`) | ⬜ Pending | ✅ Live (`Alert.alert`) | 🟡 Adapted (`Modal` confirm) |
| Symbols / Icons | ✅ Live (SF Symbols) | ⬜ Pending (text glyphs) | 🟡 Adapted (text glyphs) | 🟡 Adapted (text glyphs) |
| Shadows / Elevation | ✅ Live (`.shadow`) | ✅ Live (`shadow`, `ElevatedCard`) | ⬜ Pending | ⬜ Pending |
| Native M3 Button variants & FAB | — | ✅ Live | — | — |

---

## 4. Updating Prism as OneSim Grows

When a new OneSim capability lands (for example, `verticalScroll` or `Brush` gradients in Compose, `NavigationBar` in Compose, `scale` transforms or `expo-router` on Expo Android, or `AsyncImage` on Wasm):

1. Update the relevant cell in **Section 3** from `⬜ Pending` or `🟡 Adapted` to `✅ Live`.
2. Upgrade the corresponding screen in `examples/swiftui-prism`, `examples/compose-prism`, or `examples/expo-prism` so the real API replaces any adapted fallback.
3. Verify the web build with `scripts/build-web.sh` (or `scripts/serve-web.sh`) and confirm both light and dark modes render cleanly.
