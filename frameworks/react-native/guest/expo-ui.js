'use strict';

const PREFIX = 'ViewManagerAdapter_ExpoUI_';
const HOST = PREFIX + 'HostView';
const viewName = (component) => component.startsWith(PREFIX) ? component.slice(PREFIX.length) : null;

const views = {
  HostView: {
    props: ['matchContentsVertical', 'matchContentsHorizontal', 'alignment', 'colorScheme', 'seedColor'],
    events: ['onLayoutContent'],
  },
  TextView: {
    props: ['text', 'color', 'fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'maxLines', 'textAlign', 'typography'],
  },
  VStackView: { props: ['spacing', 'alignment'] },
  HStackView: { props: ['spacing', 'alignment'] },
  ColumnView: { props: ['verticalArrangement', 'horizontalAlignment'] },
  RowView: { props: ['horizontalArrangement', 'verticalAlignment'] },
  BoxView: { props: ['contentAlignment'] },
  Button: { props: ['label', 'enabled', 'colors'], events: ['onButtonPress', 'onButtonPressed'] },
  OutlinedButton: { props: ['enabled', 'colors'], events: ['onButtonPressed'] },
  TextButton: { props: ['enabled', 'colors'], events: ['onButtonPressed'] },
  ToggleView: { props: ['label', 'isOn'], events: ['onIsOnChange'] },
  SwitchView: { props: ['value', 'enabled', 'colors', 'nativeClickable'], events: ['onCheckedChange'] },
  SliderView: { props: ['value', 'min', 'max', 'step', 'steps', 'enabled', 'colors'], events: ['onValueChange', 'onValueChanged', 'onEditingChanged', 'onValueChangeFinished'] },
  TextFieldView: { props: ['placeholder', 'text', 'axis', 'autoFocus', 'selection'], events: ['onTextChange', 'onFocusChange', 'onSelectionChange', 'onValueChange', 'onFocusChanged', 'onKeyboardAction'] },
  SecureFieldView: { props: ['placeholder', 'text', 'autoFocus'], events: ['onTextChange', 'onFocusChange'] },
  BasicTextFieldView: { props: ['value', 'autoFocus', 'readOnly', 'enabled', 'visualTransformation', 'keyboardOptions', 'singleLine', 'maxLength', 'selection', 'textStyle'], events: ['onValueChange', 'onFocusChanged', 'onSelectionChange', 'onKeyboardAction'] },
  SlotView: { props: ['slotName'] },
  PlaceholderView: { props: [] },
  InnerTextFieldView: { props: [] },
  RNHostView: { props: ['matchContentsHorizontal', 'matchContentsVertical'] },
  ScrollViewComponent: { props: ['axes', 'showsIndicators'] },
};
for (const view of Object.values(views)) {
  view.props.push('modifiers');
  view.events = [...(view.events ?? []), 'onGlobalEvent'];
}

const palettes = {
  // Android 17 Pixel 10a default Material You palette.
  light: { primary: '#4C5E8B', onPrimary: '#F9F8FF', onSurface: '#30323A', onSurfaceVariant: '#464850', outline: '#787A84', outlineVariant: '#C8C9D4', surfaceContainerHighest: '#E1E2ED', secondaryContainer: '#DBE2F9' },
  dark: { primary: '#D0BCFF', onPrimary: '#381E72', onSurface: '#E6E1E5', onSurfaceVariant: '#CAC4D0', outline: '#938F99', outlineVariant: '#49454F', surfaceContainerHighest: '#36343B', secondaryContainer: '#4A4458' },
};

const fieldNames = new Set(['TextFieldView', 'SecureFieldView', 'BasicTextFieldView']);
const isField = (component) => fieldNames.has(viewName(component));
const isSlider = (component) => viewName(component) === 'SliderView';

function moduleConstants(cfg, rt, SharedObject) {
  const state = new Map();
  rt.expoUIState = state;
  let next = 0;
  class ObservableState extends SharedObject {
    constructor({ value }) {
      super();
      Object.defineProperty(this, '__expo_shared_object_id__', { value: ++next });
      this.__expo_ui_shared_object__ = true;
      this.current = value;
      state.set(next, this);
    }
    getValue() { return this.current; }
    setValue({ value }) {
      if (Object.is(this.current, value)) return;
      this.current = value;
      rt.scheduleEmit();
    }
    setOnChange(callback) {
      if (callback) throw new Error('OneSim: Expo UI observable worklet callbacks are not supported');
    }
    release() { state.delete(this.__expo_shared_object_id__); }
  }
  const fieldMethods = {
    focus() { rt.requestFocus(this.nativeTag); },
    blur() { if (rt.focused === this.nativeTag) rt.requestFocus(null); },
  };
  return {
    ViewPrototypes: Object.fromEntries([...fieldNames].map((name) => ['ExpoUI_' + name, fieldMethods])),
    ObservableState,
    SwitchDefaultIconSize: 16,
    isDynamicColorAvailable: false,
    getMaterialColors: (options) => Object.fromEntries(
      Object.entries(palettes[options?.scheme ?? cfg.appearance]).map(([key, value]) => [key, value + 'FF'])
    ),
  };
}

function rgba(color) {
  color = color?.color ?? color;
  if (typeof color === 'number') {
    return [color >>> 16 & 255, color >>> 8 & 255, color & 255, color >>> 24].map((v) => v / 255);
  }
  if (typeof color !== 'string') return undefined;
  if (!/^#[\da-f]{6}([\da-f]{2})?$/i.test(color)) return undefined;
  return (color.length === 7 ? color + 'FF' : color).slice(1).match(/../g).map((v) => parseInt(v, 16) / 255);
}

function swiftColor(color) {
  const channels = rgba(color);
  if (!channels) return undefined;
  const [r, g, b, a] = channels;
  return { r, g, b, a };
}

const find = (p, type) => p.modifiers?.findLast((m) => m.$type === type);
const disabled = (p) => p.enabled === false || find(p, 'disabled')?.disabled === true;
const swiftText = (id, text = '') => ({ kind: 'text', id, text });
const compose = (id, component, props, children = []) => ({ kind: 'compose', id, component, props, children });

function nestedText(view) {
  return view.props.text ?? view.children.map(nestedText).filter((v) => v != null).join('');
}

function placeholder(p, children) {
  const prompt = children.find((v) => v.props.slotName === 'prompt');
  const androidPlaceholder = children.flatMap((v) => v.children).flatMap((v) => v.children)
    .find((v) => viewName(v.component) === 'PlaceholderView');
  return p.placeholder ?? nestedText(prompt ?? androidPlaceholder ?? { props: {}, children: [] });
}

function swiftModifiers(p, id) {
  return (p.modifiers ?? []).flatMap((m) => {
    const { $type, ...args } = m;
    switch ($type) {
      case 'frame':
        return [{ frame: Object.fromEntries(Object.entries(args).map(([k, v]) => [k, v === Infinity ? 'inf' : v])) }];
      case 'font':
        return [{ font: m.textStyle && m.size == null ? { style: m.textStyle, weight: m.weight } : { size: m.size ?? 17, weight: m.weight } }];
      case 'padding':
        return [{ padding: Object.fromEntries(['top', 'trailing', 'bottom', 'leading'].map((edge) => [
          edge, m[edge] ?? (edge === 'top' || edge === 'bottom' ? m.vertical : m.horizontal) ?? m.all ?? 16,
        ])) }];
      case 'foregroundColor':
      case 'foregroundStyle': {
        const color = swiftColor(m.color ?? m.style);
        return color ? [{ foregroundColor: color }] : [];
      }
      case 'background': {
        const color = swiftColor(m.style);
        return color ? [{ background: { kind: 'color', id: id + '#background', color } }] : [];
      }
      case 'opacity': return [{ opacity: m.value }];
      default: return [];
    }
  });
}

function composeModifiers(p, id) {
  return (p.modifiers ?? []).flatMap((m) => {
    switch (m.$type) {
      case 'padding': return [{ padding: [m.start ?? 0, m.top ?? 0, m.end ?? 0, m.bottom ?? 0] }];
      case 'paddingAll': return [{ padding: Array(4).fill(m.all) }];
      case 'size': return [{ width: m.width }, { height: m.height }];
      case 'width': return typeof m.width === 'number' ? [{ width: m.width }] : [];
      case 'height': return [{ height: m.height }];
      case 'fillMaxWidth': return [{ fillMaxWidth: m.fraction ?? 1 }];
      case 'fillMaxHeight': return [{ fillMaxHeight: m.fraction ?? 1 }];
      case 'verticalScroll': return [{ scroll: true }];
      case 'horizontalScroll': return [{ scroll: false }];
      case 'defaultMinSize': return [{ defaultMinSize: [m.minWidth, m.minHeight] }];
      case 'weight': return [{ weight: m.weight }];
      case 'alpha': return [{ alpha: m.alpha }];
      case 'background': return [{ background: { color: rgba(m.color) } }];
      case 'toggleable':
      case 'clickable':
        return [{ clickable: { action: Number(id), role: m.role, enabled: m.enabled !== false } }];
      default: return [];
    }
  });
}

const bias = (alignment) => ({ start: -1, top: -1, end: 1, bottom: 1 }[alignment] ?? 0);
const arrangement = (a) => {
  if (typeof a === 'object' && a != null) return { type: 'spacedBy', space: a.spacedBy };
  return { type: a ?? 'start' };
};

function composeButton(id, name, p, children, palette, modifier) {
  const filled = name === 'Button';
  const outlined = name === 'OutlinedButton';
  const horizontal = name === 'TextButton' ? 12 : 24;
  const enabled = !disabled(p);
  const circle = { percent: [50, 50, 50, 50] };
  const fill = enabled ? rgba(p.colors?.containerColor ?? palette.primary) : [...rgba(palette.onSurface).slice(0, 3), 0.1];
  const foreground = enabled
    ? rgba(p.colors?.contentColor ?? (filled ? palette.onPrimary : palette.primary))
    : [...rgba(palette.onSurfaceVariant).slice(0, 3), 0.38];
  const center = { type: 'center' };
  return compose(id, 'Box', {
    contentAlignment: [0, 0],
    modifier: [
      ...modifier,
      { minInteractive: 48 },
      { clickable: { action: Number(id), enabled } },
      ...(filled ? [{ background: { color: fill, shape: circle } }] : []),
      ...(outlined ? [{ border: { width: 1, color: rgba(palette.outlineVariant), shape: circle } }] : []),
    ],
  }, [compose(id + '#labelRow', 'Row', {
    arrangement: center, alignment: 0,
    modifier: [{ defaultMinSize: [58, 40] }, { padding: [horizontal, 8, horizontal, 8] }],
  }, children.map((child) => child.component === 'Text' ? {
    ...child, props: { fontSize: 14, fontWeight: 500, lineHeight: 20, ...child.props, color: child.props.color ?? foreground },
  } : child))]);
}

function composeSwitch(id, p, palette, modifier) {
  const on = p.value === true;
  const enabled = !disabled(p);
  const keys = on ? ['checkedTrackColor', 'checkedThumbColor'] : ['uncheckedTrackColor', 'uncheckedThumbColor', 'uncheckedBorderColor'];
  const fallback = on ? [palette.primary, palette.onPrimary] : [palette.surfaceContainerHighest, palette.outline, palette.outline];
  const [track, thumb, border] = keys.map((key, i) => rgba(p.colors?.[key] ?? fallback[i]));
  const circle = { percent: [50, 50, 50, 50] };
  const d = on ? 24 : 16;
  const dx = on ? 24 : 8;
  const click = p.nativeClickable ? [
    { minInteractive: 48 }, { clickable: { action: Number(id), role: 'switch', enabled } },
  ] : [];
  const core = compose(id + '#track', 'Box', {
    contentAlignment: [-1, 0],
    modifier: [
      ...click, { width: 52 }, { height: 32 }, { background: { color: track, shape: circle } },
      ...(border ? [{ border: { width: 2, color: border, shape: circle } }] : []),
    ],
  }, [compose(id + '#thumb', 'Box', {
    modifier: [{ offset: [dx, 0] }, { width: d }, { height: d }, { background: { color: thumb, shape: circle } }],
  })]);
  return compose(id, 'Box', { contentAlignment: [0, 0], modifier: [...modifier, ...(!enabled ? [{ alpha: 0.38 }] : [])] }, [core]);
}

function nativeTree(host, android, appearance) {
  const palette = palettes[host.props.colorScheme === 'dark' || (host.props.colorScheme !== 'light' && appearance === 'dark') ? 'dark' : 'light'];
  const convert = (view) => {
    const { id, component, props: p, children } = view;
    const name = viewName(component);
    const kids = () => children.map(convert);
    if (name === 'RNHostView') {
      return android
        ? compose(id + '#container', 'Box', { modifier: composeModifiers(p, id) }, [view])
        : { ...view, modifiers: swiftModifiers(p, id) };
    }
    if (android) {
      const modifier = composeModifiers(p, id);
      if (name === 'TextView') {
        const defaults = p.typography === 'labelLarge' ? { fontSize: 14, fontWeight: 500, lineHeight: 20 } : {};
        const weight = Number(p.fontWeight);
        const props = { ...defaults, ...p, modifier, color: rgba(p.color) };
        if (weight > 0) props.fontWeight = weight;
        return compose(id, 'Text', props);
      }
      if (name === 'ColumnView' || name === 'RowView') {
        const column = name === 'ColumnView';
        return compose(id, column ? 'Column' : 'Row', {
          modifier, arrangement: arrangement(column ? p.verticalArrangement : p.horizontalArrangement),
          alignment: bias(column ? p.horizontalAlignment ?? 'start' : p.verticalAlignment ?? 'top'),
        }, kids());
      }
      if (name === 'BoxView') {
        const align = p.contentAlignment ?? 'topStart';
        return compose(id, 'Box', {
          modifier, contentAlignment: [align.endsWith('Start') ? -1 : align.endsWith('End') ? 1 : 0, align.startsWith('top') ? -1 : align.startsWith('bottom') ? 1 : 0],
        }, kids());
      }
      if (['Button', 'TextButton', 'OutlinedButton'].includes(name)) return composeButton(id, name, p, kids(), palette, modifier);
      if (name === 'SwitchView') return composeSwitch(id, p, palette, modifier);
      if (name === 'SliderView') return compose(id, 'Slider', {
        modifier, min: p.min, max: p.max, value: p.value, field: Number(id), enabled: !disabled(p),
        activeColor: rgba(p.colors?.activeTrackColor ?? palette.primary), inactiveColor: rgba(p.colors?.inactiveTrackColor ?? palette.secondaryContainer),
      });
      if (fieldNames.has(name)) return compose(id, 'TextField', {
        modifier, plain: name === 'BasicTextFieldView', field: Number(id), text: p.__simcoreText,
        placeholder: placeholder(p, children), enabled: p.enabled !== false && p.readOnly !== true,
        secure: p.visualTransformation === 'password', fontSize: p.textStyle?.fontSize, color: rgba(p.textStyle?.color),
      });
    } else {
      const modifiers = swiftModifiers(p, id);
      const label = () => children.length ? convert(children[0]) : swiftText(id + '#label', p.label);
      switch (name) {
        case 'TextView': return { ...swiftText(id, p.text), modifiers };
        case 'ScrollViewComponent':
          return { kind: 'scroll', id, modifiers, axis: p.axes === 'horizontal' ? 'horizontal' : 'vertical', child: convert(children[0] ?? { id: id + '#empty', component: '', props: {}, children: [] }) };
        case 'VStackView':
        case 'HStackView': return { kind: name === 'VStackView' ? 'vstack' : 'hstack', id, modifiers, spacing: p.spacing, alignment: p.alignment, children: kids() };
        case 'Button': {
          const style = find(p, 'buttonStyle')?.style;
          return { kind: 'button', id, modifiers, label: label(), action: Number(id), style: style === 'borderless' ? 'automatic' : style, disabled: disabled(p) };
        }
        case 'ToggleView': return { kind: 'toggle', id, modifiers, label: label(), action: Number(id), isOn: p.isOn === true, disabled: disabled(p) };
        case 'SliderView':
          return { kind: 'slider', id, modifiers, value: p.value ?? 0, min: p.min, max: p.max, field: Number(id), disabled: disabled(p) };
        case 'TextFieldView':
        case 'SecureFieldView':
          return { kind: 'text_field', id, modifiers, placeholder: placeholder(p, children), text: p.__simcoreText, field: Number(id), secure: name === 'SecureFieldView', disabled: disabled(p) };
      }
    }
    return { kind: 'empty', id };
  };
  const id = host.id + '#native';
  const children = host.children.map(convert);
  return android
    ? compose(id, 'Box', { modifier: composeModifiers(host.props, host.id) }, children)
    : { kind: 'zstack', id, alignment: host.props.alignment ?? 'topLeading', modifiers: swiftModifiers(host.props, id), children };
}

function action(node, platform, dispatch) {
  const p = node.props;
  switch (viewName(node.viewName)) {
    case 'Button':
    case 'TextButton':
    case 'OutlinedButton':
      dispatch(platform === 'ios' ? 'topButtonPress' : 'topButtonPressed', {});
      break;
    case 'ToggleView': dispatch('topIsOnChange', { isOn: !p.isOn }); break;
    case 'SwitchView': dispatch('topCheckedChange', { value: !p.value }); break;
    default: {
      const mod = p.modifiers?.find((m) => m.$type === 'clickable' || m.$type === 'toggleable');
      if (!mod) return false;
      dispatch('topGlobalEvent', { payload: [mod.$type, {}] });
    }
  }
  return true;
}

module.exports = { HOST, viewName, views, isField, isSlider, moduleConstants, nativeTree, action };
