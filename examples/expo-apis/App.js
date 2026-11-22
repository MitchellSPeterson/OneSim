// React Native / Expo APIs without a showcase screen of their own (issue #50): LayoutAnimation, ActionSheetIOS,
// GIF / WebP images (core Image and expo-image), ActivityIndicator, Linking and expo-font. Reference screenshots:
// tests/fixtures/ios27/rn-apis-ios (iOS 27.2) and tests/fixtures/rn-apis-ios (iOS 26.5).
import { useEffect, useState } from 'react';
import {
  ActionSheetIOS, ActivityIndicator, Image, LayoutAnimation, Linking, Pressable, ScrollView, StyleSheet, Text, View, useColorScheme,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';
import { Image as ExpoImage } from 'expo-image';
import * as Font from 'expo-font';
import * as ExpoLinking from 'expo-linking';

const SCREENS = ['Layout', 'Sheet', 'Images', 'Fonts', 'Links'];

function Header({ title, onBack }) {
  return (
    <View style={styles.header}>
      <Pressable accessibilityLabel="Back" onPress={onBack} style={styles.back}>
        <Text style={styles.link}>Back</Text>
      </Pressable>
      <Text style={styles.headerTitle}>{title}</Text>
      <View style={styles.back} />
    </View>
  );
}

function Button({ label, onPress }) {
  return (
    <Pressable accessibilityLabel={label} onPress={onPress} style={styles.button}>
      <Text style={styles.buttonText}>{label}</Text>
    </Pressable>
  );
}

// LayoutAnimation: Toggle grows the card and inserts a row (easeInEaseOut), Shrink takes them back with a spring.
function LayoutScreen({ fg }) {
  const [open, setOpen] = useState(false);
  const [ended, setEnded] = useState(0);
  const toggle = (preset) => {
    LayoutAnimation.configureNext(preset, () => setEnded((n) => n + 1));
    setOpen((o) => !o);
  };
  return (
    <View style={styles.body}>
      <View style={styles.row}>
        <Button label="Toggle" onPress={() => toggle(LayoutAnimation.Presets.easeInEaseOut)} />
        <Button label="Spring" onPress={() => toggle(LayoutAnimation.Presets.spring)} />
      </View>
      <View style={[styles.card, { height: open ? 180 : 60 }]}>
        <Text style={styles.cardText}>{open ? 'Expanded' : 'Collapsed'}</Text>
      </View>
      {open && <View style={styles.extra}><Text style={styles.cardText}>Inserted row</Text></View>}
      <View style={styles.footer} />
      <Text accessibilityLabel="Ended" style={[styles.note, { color: fg }]}>{`Animations ended: ${ended}`}</Text>
    </View>
  );
}

function SheetScreen({ fg }) {
  const [picked, setPicked] = useState('none');
  const show = () => ActionSheetIOS.showActionSheetWithOptions(
    { options: ['Cancel', 'Delete photo', 'Save to Files'], destructiveButtonIndex: 1, cancelButtonIndex: 0, title: 'Photo', message: 'Choose what to do with this photo' },
    (i) => setPicked(String(i)),
  );
  return (
    <View style={styles.body}>
      <Button label="Show sheet" onPress={show} />
      <Text accessibilityLabel="Picked" style={[styles.note, { color: fg }]}>{`Picked: ${picked}`}</Text>
    </View>
  );
}

// Still GIF / WebP through core Image and expo-image, two animated ones and the spinners.
function ImagesScreen({ fg }) {
  return (
    <ScrollView style={styles.body}>
      <Text style={[styles.caption, { color: fg }]}>Image: GIF, WebP</Text>
      <View style={styles.row}>
        <Image source={require('./assets/checker.gif')} style={styles.pic} />
        <Image source={require('./assets/sunset.webp')} style={[styles.pic, { borderRadius: 16 }]} />
      </View>
      <Text style={[styles.caption, { color: fg }]}>expo-image: WebP, GIF</Text>
      <View style={styles.row}>
        <ExpoImage source={require('./assets/badge.webp')} style={styles.badge} contentFit="contain" />
        <ExpoImage source={require('./assets/checker.gif')} style={styles.pic} contentFit="cover" />
      </View>
      <Text style={[styles.caption, { color: fg }]}>Animated GIF, WebP and spinners</Text>
      <View style={[styles.row, { alignItems: 'center' }]}>
        <Image source={require('./assets/pulse.gif')} style={styles.anim} />
        <ExpoImage source={require('./assets/bars.webp')} style={styles.anim} />
        <ActivityIndicator style={styles.spin} />
        <ActivityIndicator size="large" color="#AF52DE" style={styles.spin} />
      </View>
    </ScrollView>
  );
}

function FontsScreen({ fg }) {
  const [loaded, setLoaded] = useState(Font.isLoaded('Roboto'));
  useEffect(() => {
    Font.loadAsync({ Roboto: require('./assets/Roboto.ttf') }).then(() => setLoaded(Font.isLoaded('Roboto')), (e) => setLoaded(`error ${e.message}`));
  }, []);
  return (
    <View style={styles.body}>
      <Text style={[styles.note, { color: fg }]}>{`Loaded: ${loaded}`}</Text>
      {loaded === true && (
        <View>
          <Text style={{ fontFamily: 'Roboto', fontSize: 34, color: fg, marginTop: 12 }}>Roboto 34</Text>
          <Text style={{ fontFamily: 'Roboto', fontSize: 17, color: fg, marginTop: 8 }}>The quick brown fox jumps over the lazy dog</Text>
          <Text style={{ fontSize: 17, color: fg, marginTop: 8 }}>System: the quick brown fox jumps over the lazy dog</Text>
          <Text style={{ fontSize: 17, color: fg, marginTop: 8 }}>Mixed <Text style={{ fontFamily: 'Roboto', color: '#007AFF' }}>Roboto span</Text> in system text</Text>
        </View>
      )}
    </View>
  );
}

function LinksScreen({ fg }) {
  const [initial, setInitial] = useState('…');
  const [last, setLast] = useState('none');
  const [result, setResult] = useState('none');
  useEffect(() => {
    Linking.getInitialURL().then((u) => setInitial(String(u)));
    const sub = Linking.addEventListener('url', (e) => setLast(e.url));
    return () => sub.remove();
  }, []);
  const run = (what, p) => p.then((v) => setResult(`${what} ${v}`), (e) => setResult(`${what} failed ${e.message}`));
  return (
    <View style={styles.body}>
      <Text style={[styles.note, { color: fg }]}>{`Initial: ${initial}`}</Text>
      <Text style={[styles.note, { color: fg }]}>{`Last: ${last}`}</Text>
      <Text style={[styles.note, { color: fg }]}>{`Result: ${result}`}</Text>
      <View style={styles.wrap}>
        <Button label="Open self" onPress={() => run('self', Linking.openURL(ExpoLinking.createURL('done', { queryParams: { n: '1' } })))} />
        <Button label="Open web" onPress={() => run('web', Linking.openURL('https://expo.dev'))} />
        <Button label="Can tel" onPress={() => run('tel', Linking.canOpenURL('tel:5551234'))} />
        <Button label="Can odd" onPress={() => run('odd', Linking.canOpenURL('notinstalled-app://x'))} />
        <Button label="Settings" onPress={() => run('settings', Linking.openSettings())} />
        <Button label="Bad URL" onPress={() => run('bad', Linking.openURL('no scheme'))} />
      </View>
    </View>
  );
}

export default function App() {
  const [screen, setScreen] = useState(null);
  const dark = useColorScheme() === 'dark';
  const fg = dark ? '#fff' : '#000';
  const Body = { Layout: LayoutScreen, Sheet: SheetScreen, Images: ImagesScreen, Fonts: FontsScreen, Links: LinksScreen }[screen];
  return (
    <View style={[styles.root, { backgroundColor: dark ? '#000' : '#fff' }]}>
      <StatusBar style={dark ? 'light' : 'dark'} />
      {screen === null ? (
        <View style={{ flex: 1 }}>
          <Text style={[styles.title, { color: fg }]}>APIs</Text>
          {SCREENS.map((s) => (
            <Pressable key={s} accessibilityLabel={s} onPress={() => setScreen(s)}
              style={[styles.homeRow, { borderBottomColor: dark ? '#38383A' : '#E5E5EA' }]}>
              <Text style={{ fontSize: 17, color: fg }}>{s}</Text>
              <Text style={{ fontSize: 17, color: '#C7C7CC' }}>{'>'}</Text>
            </Pressable>
          ))}
        </View>
      ) : (
        <View style={{ flex: 1 }}>
          <Header title={screen} onBack={() => setScreen(null)} />
          <Body fg={fg} />
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
  row: { flexDirection: 'row', marginBottom: 12 },
  wrap: { flexDirection: 'row', flexWrap: 'wrap', marginTop: 12 },
  button: { backgroundColor: '#007AFF', paddingHorizontal: 20, paddingVertical: 12, borderRadius: 12, marginRight: 8, marginBottom: 8 },
  buttonText: { color: '#fff', fontSize: 17, fontWeight: '600' },
  card: { backgroundColor: '#5856D6', borderRadius: 16, justifyContent: 'center', paddingHorizontal: 16, marginTop: 4 },
  extra: { backgroundColor: '#34C759', borderRadius: 16, height: 60, justifyContent: 'center', paddingHorizontal: 16, marginTop: 12 },
  cardText: { color: '#fff', fontSize: 20, fontWeight: '600' },
  footer: { backgroundColor: '#FF9500', borderRadius: 16, height: 60, marginTop: 12 },
  note: { fontSize: 17, marginTop: 16 },
  caption: { fontSize: 15, marginBottom: 8 },
  pic: { width: 120, height: 80, marginRight: 16 },
  badge: { width: 80, height: 80, marginRight: 16 },
  anim: { width: 40, height: 40, marginRight: 24 },
  spin: { marginRight: 24 },
});
