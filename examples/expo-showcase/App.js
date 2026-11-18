import { useRef, useState } from 'react';
import {
  FlatList, Image, Pressable, ScrollView, StyleSheet, Switch, Text, TextInput, View, useColorScheme,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';

const SCREENS = ['Layout', 'Text', 'Controls', 'Scroll', 'Image'];

function Header({ title, onBack, children }) {
  return (
    <View style={styles.header}>
      <Pressable accessibilityLabel="Back" onPress={onBack} style={styles.back}>
        <Text style={styles.link}>Back</Text>
      </Pressable>
      <Text style={styles.headerTitle}>{title}</Text>
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
      <Text style={styles.big}>Count: {count}</Text>
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

export default function App() {
  const [screen, setScreen] = useState(null);
  const dark = useColorScheme() === 'dark';
  const bg = dark ? '#000' : '#fff';
  const fg = dark ? '#fff' : '#000';
  const back = () => setScreen(null);
  const Body = { Layout, Text: TextScreen, Controls, Image: ImageScreen }[screen];
  return (
    <View style={[styles.root, { backgroundColor: bg }]}>
      <StatusBar style={dark ? 'light' : 'dark'} />
      {screen === null && (
        <View style={{ flex: 1 }}>
          <Text style={[styles.title, { color: fg }]}>Showcase</Text>
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
      {screen !== null && screen !== 'Scroll' && (
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
  listRow: { height: 44, justifyContent: 'center', paddingHorizontal: 16, borderBottomWidth: 1, borderBottomColor: '#E5E5EA' },
});
