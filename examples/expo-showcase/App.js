import { useRef, useState } from 'react';
import {
  ActivityIndicator, Alert, FlatList, LogBox, Image, KeyboardAvoidingView, Pressable, RefreshControl, SafeAreaView, ScrollView, StyleSheet, Switch, Text,
  TextInput, View, useColorScheme,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';
import { LinearGradient } from 'expo-linear-gradient';
import { BlurView } from 'expo-blur';

// iOS Expo Go sends console warnings to the debugger (Fusebox), OneSim to LogBox's toast: keep the core SafeAreaView
// deprecation warning (RN 0.88) off the screenshots.
LogBox.ignoreLogs(['SafeAreaView has been deprecated']);

const SCREENS = ['Layout', 'Text', 'Controls', 'Scroll', 'Image', 'Visuals', 'Refresh'];
const STRIPES = ['#FF3B30', '#FFCC00', '#34C759', '#007AFF', '#AF52DE', '#FF9500', '#5AC8FA', '#FF2D55',
  '#FF3B30', '#FFCC00', '#34C759', '#007AFF', '#AF52DE', '#FF9500', '#5AC8FA'];

function Header({ title, onBack, onTitlePress, children }) {
  const label = <Text style={styles.headerTitle}>{title}</Text>;
  return (
    <View style={styles.header}>
      <Pressable accessibilityLabel="Back" onPress={onBack} style={styles.back}>
        <Text style={styles.link}>Back</Text>
      </Pressable>
      {onTitlePress ? <Pressable accessibilityLabel="Status Bar" onPress={onTitlePress}>{label}</Pressable> : label}
      {children}
    </View>
  );
}

function Layout() {
  return (
    <ScrollView style={styles.body}>
      <View style={styles.row}>
        <View style={[styles.box, { backgroundColor: '#FF3B30' }]} />
        <View style={[styles.box, { backgroundColor: '#34C759' }]} />
        <View style={[styles.box, { backgroundColor: '#007AFF' }]} />
      </View>
      <View style={[styles.row, { justifyContent: 'space-between', marginTop: 12 }]}>
        <View style={[styles.box, { backgroundColor: '#FF9500' }]} />
        <View style={[styles.box, { backgroundColor: '#AF52DE' }]} />
        <View style={[styles.box, { backgroundColor: '#5AC8FA' }]} />
      </View>
      <View style={{ flexDirection: 'row', marginTop: 12, height: 90 }}>
        <View style={{ flex: 1, backgroundColor: '#FFD60A', marginRight: 8 }} />
        <View style={{ flex: 2, backgroundColor: '#8E8E93', marginRight: 8 }} />
        <View style={{ flexDirection: 'column', flex: 1, justifyContent: 'space-between' }}>
          <View style={{ height: 40, backgroundColor: '#FF2D55' }} />
          <View style={{ height: 40, backgroundColor: '#30B0C7' }} />
        </View>
      </View>
      <View style={styles.wrap}>
        {[1, 2, 3, 4, 5, 6, 7].map((i) => (
          <View key={i} style={styles.chip}><Text style={styles.chipText}>Tag {i}</Text></View>
        ))}
      </View>
      <View style={styles.bordered}><Text style={styles.chipText}>Border 3, radius 16</Text></View>
      <View style={styles.pct}>
        <View style={{ width: '50%', height: 20, backgroundColor: '#007AFF' }} />
        <View style={{ width: '25%', height: 20, backgroundColor: '#34C759', marginTop: 6 }} />
        <View style={{ width: '75%', height: 20, backgroundColor: '#FF9500', marginTop: 6, opacity: 0.5 }} />
      </View>
      <View style={styles.abs}>
        <View style={{ position: 'absolute', top: 10, left: 10, width: 60, height: 60, borderRadius: 30, backgroundColor: '#FF3B30' }} />
        <View style={{ position: 'absolute', bottom: 10, right: 10, width: 80, height: 40, backgroundColor: '#007AFF', opacity: 0.7 }} />
        <View style={{ position: 'absolute', top: 30, left: 50, width: 80, height: 50, backgroundColor: '#34C759', opacity: 0.6, borderRadius: 8 }} />
      </View>
    </ScrollView>
  );
}

function TextScreen() {
  return (
    <ScrollView style={styles.body}>
      {[11, 13, 15, 17, 20, 24, 28, 34].map((s) => (
        <Text key={s} style={{ fontSize: s, color: '#000' }}>Size {s}</Text>
      ))}
      <Text style={{ fontSize: 17, fontWeight: '400' }}>Regular 400</Text>
      <Text style={{ fontSize: 17, fontWeight: '600' }}>Semibold 600</Text>
      <Text style={{ fontSize: 17, fontWeight: '700' }}>Bold 700</Text>
      <Text style={{ fontSize: 17, textAlign: 'center' }}>Centered</Text>
      <Text style={{ fontSize: 17, textAlign: 'right' }}>Right aligned</Text>
      <Text style={{ fontSize: 15, lineHeight: 26, marginTop: 8 }}>
        Line height 26: the quick brown fox jumps over the lazy dog and keeps running far away.
      </Text>
      <Text style={{ fontSize: 17, marginTop: 8 }}>
        Plain <Text style={{ color: '#FF3B30' }}>red</Text> <Text style={{ color: '#007AFF', fontWeight: '700' }}>blue bold</Text> <Text style={{ color: '#34C759' }}>green</Text> text.
      </Text>
      <Text numberOfLines={1} style={{ fontSize: 17, marginTop: 8 }}>
        One line only: this long sentence must be truncated with an ellipsis at the end.
      </Text>
      <Text numberOfLines={2} style={{ fontSize: 17, marginTop: 8 }}>
        Two lines max: this much longer sentence keeps going and going so that the second line is cut off with an ellipsis once it runs out of room on the screen.
      </Text>
    </ScrollView>
  );
}

function Controls() {
  const [count, setCount] = useState(0);
  const [a, setA] = useState(true);
  const [b, setB] = useState(false);
  const [text, setText] = useState('');
  return (
    <View style={styles.body}>
      {/* Tapping the count asks to reset it (Alert.alert; adds no visible element) */}
      <Pressable accessibilityLabel="Count" onPress={() => Alert.alert('Reset count?', 'The count goes back to zero.', [
        { text: 'Cancel', style: 'cancel' },
        { text: 'Reset', style: 'destructive', onPress: () => setCount(0) },
      ])}>
        <Text style={styles.big}>Count: {count}</Text>
      </Pressable>
      <Pressable accessibilityLabel="Increment" onPress={() => setCount((c) => c + 1)}
        style={({ pressed }) => [styles.button, pressed && { opacity: 0.6 }]}>
        <Text style={styles.buttonText}>Increment</Text>
      </Pressable>
      <View style={styles.switchRow}>
        <Text style={styles.label}>Alpha</Text>
        <Switch accessibilityLabel="Alpha" value={a} onValueChange={setA} />
      </View>
      <View style={styles.switchRow}>
        <Text style={styles.label}>Beta</Text>
        <Switch accessibilityLabel="Beta" value={b} onValueChange={setB} />
      </View>
      <TextInput accessibilityLabel="Type here" placeholder="Type here" placeholderTextColor="#8E8E93"
        value={text} onChangeText={setText} style={styles.input} />
    </View>
  );
}

// Reached by tapping the home title (no visible entry, so the other screens' references stay valid).
// keyboardVerticalOffset 62 = root paddingTop: KeyboardAvoidingView compares its parent-relative frame with screen coordinates.
function Form() {
  const [text, setText] = useState('');
  return (
    <KeyboardAvoidingView behavior="padding" keyboardVerticalOffset={62} style={styles.form}>
      <Text style={styles.label}>Message</Text>
      <TextInput accessibilityLabel="Message" placeholder="Message" placeholderTextColor="#8E8E93"
        value={text} onChangeText={setText} style={styles.input} />
    </KeyboardAvoidingView>
  );
}

function Scroll({ onBack }) {
  const ref = useRef(null);
  return (
    <View style={{ flex: 1 }}>
      <Header title="Scroll" onBack={onBack}>
        <Pressable accessibilityLabel="Jump" onPress={() => ref.current?.scrollToOffset({ offset: 440, animated: false })}>
          <Text style={styles.link}>Jump</Text>
        </Pressable>
      </Header>
      <FlatList
        ref={ref}
        data={Array.from({ length: 50 }, (_, i) => `Row ${i + 1}`)}
        keyExtractor={(t) => t}
        getItemLayout={(_, i) => ({ length: 44, offset: 44 * i, index: i })}
        renderItem={({ item }) => (
          <View style={styles.listRow}><Text style={styles.label}>{item}</Text></View>
        )}
      />
    </View>
  );
}

function ImageScreen() {
  const src = require('./assets/gradient.png');
  return (
    <ScrollView style={styles.body}>
      <Image source={src} style={{ width: 300, height: 100, backgroundColor: '#EEE' }} resizeMode="cover" />
      <Image source={src} style={{ width: 300, height: 100, marginTop: 12, backgroundColor: '#EEE' }} resizeMode="contain" />
      <Image source={src} style={{ width: 200, height: 120, marginTop: 12, borderRadius: 24 }} />
    </ScrollView>
  );
}

// Root has no paddingTop here: the core SafeAreaView pads by the device insets (62 top, 34 bottom).
function Visuals({ onBack, onStatusBar, fg }) {
  return (
    <SafeAreaView style={{ flex: 1 }}>
      <Header title="Visuals" onBack={onBack} onTitlePress={onStatusBar} />
      <View style={styles.body}>
        <View style={styles.spinRow}>
          <ActivityIndicator />
          <ActivityIndicator size="large" />
          <ActivityIndicator color="#FF3B30" />
          <ActivityIndicator size="large" color="#007AFF" />
          <ActivityIndicator size="large" animating={false} />
        </View>
        <LinearGradient colors={['#FF9500', '#FF2D55', '#5856D6']} locations={[0, 0.3, 1]}
          start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }} style={styles.gradient}>
          <Text style={styles.gradientText}>Gradient</Text>
        </LinearGradient>
        <LinearGradient colors={['#34C759', '#007AFF']} start={{ x: 0, y: 0.5 }} end={{ x: 1, y: 0.5 }} style={styles.pill} />
        <View style={styles.blurBox}>
          {STRIPES.map((c, i) => <View key={i} style={{ flex: 1, backgroundColor: c }} />)}
          <BlurView style={[styles.tile, { left: 8 }]} />
          <BlurView tint="light" intensity={100} style={[styles.tile, { left: 98 }]} />
          <BlurView tint="dark" intensity={100} style={[styles.tile, { left: 188 }]} />
          <BlurView tint="systemMaterial" intensity={100} style={[styles.tile, { left: 278 }]} />
        </View>
        <Text style={[styles.label, { color: fg, marginTop: 12 }]}>Blur: default, light, dark, material</Text>
      </View>
    </SafeAreaView>
  );
}

function StatusBarScreen({ onBack }) {
  const [mode, setMode] = useState('Dark Content');
  const light = mode === 'Dark Content';
  return (
    <View style={{ flex: 1, paddingTop: 62, backgroundColor: light ? '#e5e5ea' : '#242426' }}>
      <StatusBar style={light ? 'dark' : 'light'} hidden={mode === 'Hidden'} />
      <Header title="Status Bar" onBack={onBack} />
      <View style={styles.body}>
        {['Dark Content', 'Light Content', 'Hidden'].map((choice) => (
          <Pressable key={choice} accessibilityLabel={choice} onPress={() => setMode(choice)} style={styles.listRow}>
            <Text style={[styles.label, { color: light ? '#000' : '#fff', fontWeight: mode === choice ? '600' : '400' }]}>{choice}</Text>
          </Pressable>
        ))}
      </View>
    </View>
  );
}

// Pull down past the top fires onRefresh; refreshing then stays on (spinner above the rows).
function Refresh({ onBack, fg, dark }) {
  const [refreshing, setRefreshing] = useState(false);
  return (
    <View style={{ flex: 1 }}>
      <Header title={refreshing ? 'Refreshing' : 'Refresh'} onBack={onBack} />
      <FlatList
        data={Array.from({ length: 30 }, (_, i) => `Item ${i + 1}`)}
        keyExtractor={(t) => t}
        refreshControl={<RefreshControl refreshing={refreshing} onRefresh={() => setRefreshing(true)} />}
        renderItem={({ item }) => (
          <View style={[styles.listRow, { borderBottomColor: dark ? '#38383A' : '#E5E5EA' }]}><Text style={[styles.label, { color: fg }]}>{item}</Text></View>
        )}
      />
    </View>
  );
}

export default function App() {
  const [screen, setScreen] = useState(null);
  const dark = useColorScheme() === 'dark';
  const bg = dark ? '#000' : '#fff';
  const fg = dark ? '#fff' : '#000';
  const back = () => setScreen(null);
  const Body = { Layout, Text: TextScreen, Controls, Image: ImageScreen, Form }[screen];
  return (
    <View style={[styles.root, { backgroundColor: bg }, (screen === 'Visuals' || screen === 'Status Bar') && { paddingTop: 0 }]}>
      <StatusBar style={dark ? 'light' : 'dark'} />
      {screen === null && (
        <View style={{ flex: 1 }}>
          <Pressable onPress={() => setScreen('Form')}><Text style={[styles.title, { color: fg }]}>Showcase</Text></Pressable>
          {SCREENS.map((s) => (
            <Pressable key={s} accessibilityLabel={s} onPress={() => setScreen(s)}
              style={[styles.homeRow, { borderBottomColor: dark ? '#38383A' : '#E5E5EA' }]}>
              <Text style={[styles.label, { color: fg }]}>{s}</Text>
              <Text style={{ fontSize: 17, color: '#C7C7CC' }}>{'>'}</Text>
            </Pressable>
          ))}
        </View>
      )}
      {screen === 'Scroll' && <Scroll onBack={back} />}
      {screen === 'Visuals' && <Visuals onBack={back} onStatusBar={() => setScreen('Status Bar')} fg={fg} />}
      {screen === 'Status Bar' && <StatusBarScreen onBack={back} />}
      {screen === 'Refresh' && <Refresh onBack={back} fg={fg} dark={dark} />}
      {Body && (
        <View style={{ flex: 1 }}>
          <Header title={screen} onBack={back} />
          <Body />
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, paddingTop: 62 },
  title: { fontSize: 34, fontWeight: '700', paddingHorizontal: 16, paddingVertical: 8 },
  homeRow: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', height: 52, marginHorizontal: 16, borderBottomWidth: 1 },
  header: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', height: 44, paddingHorizontal: 16 },
  back: { width: 60 },
  headerTitle: { fontSize: 17, fontWeight: '600', color: '#8E8E93' },
  link: { fontSize: 17, color: '#007AFF' },
  body: { flex: 1, padding: 16 },
  label: { fontSize: 17, color: '#000' },
  row: { flexDirection: 'row' },
  box: { width: 60, height: 60, marginRight: 8, borderRadius: 8 },
  wrap: { flexDirection: 'row', flexWrap: 'wrap', marginTop: 12 },
  chip: { backgroundColor: '#E5E5EA', borderRadius: 14, paddingHorizontal: 12, paddingVertical: 6, marginRight: 8, marginBottom: 8 },
  chipText: { fontSize: 15, color: '#000' },
  bordered: { borderWidth: 3, borderColor: '#007AFF', borderRadius: 16, padding: 12, marginTop: 4 },
  pct: { marginTop: 12 },
  abs: { height: 90, backgroundColor: '#F2F2F7', marginTop: 12, marginBottom: 24 },
  big: { fontSize: 28, fontWeight: '600', color: '#000', marginBottom: 16 },
  button: { backgroundColor: '#007AFF', alignSelf: 'flex-start', paddingHorizontal: 24, paddingVertical: 12, borderRadius: 12 },
  buttonText: { color: '#fff', fontSize: 17, fontWeight: '600' },
  switchRow: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginTop: 20 },
  input: { marginTop: 24, height: 44, borderWidth: 1, borderColor: '#C7C7CC', borderRadius: 8, paddingHorizontal: 12, fontSize: 17, color: '#000' },
  form: { flex: 1, justifyContent: 'flex-end', padding: 16 },
  spinRow: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-around', height: 60 },
  gradient: { height: 120, borderRadius: 16, marginTop: 12, justifyContent: 'center', alignItems: 'center' },
  gradientText: { color: '#fff', fontSize: 28, fontWeight: '700' },
  pill: { height: 44, borderRadius: 22, marginTop: 12 },
  blurBox: { flexDirection: 'row', height: 140, marginTop: 12, borderRadius: 16, overflow: 'hidden' },
  tile: { position: 'absolute', top: 30, width: 82, height: 80, borderRadius: 12 },
  listRow: { height: 44, justifyContent: 'center', paddingHorizontal: 16, borderBottomWidth: 1, borderBottomColor: '#E5E5EA' },
});
