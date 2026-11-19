import { useState } from 'react';
import { Pressable, StyleSheet, Text, Vibration, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import * as Haptics from 'expo-haptics';
import * as Location from 'expo-location';
import { Camera } from 'expo-camera';
import { Directory, File, Paths } from 'expo-file-system';
import * as SecureStore from 'expo-secure-store';
import AsyncStorage from '@react-native-async-storage/async-storage';
import * as Application from 'expo-application';
import * as Battery from 'expo-battery';
import * as Brightness from 'expo-brightness';
import * as Crypto from 'expo-crypto';
import * as LocalAuthentication from 'expo-local-authentication';
import * as Localization from 'expo-localization';
import * as Network from 'expo-network';
import * as ScreenOrientation from 'expo-screen-orientation';
import * as StoreReview from 'expo-store-review';
import * as Tracking from 'expo-tracking-transparency';

const fmt = (l) => `lat ${l.coords.latitude} lon ${l.coords.longitude}`;

const Button = ({ label, onPress }) => (
  <Pressable accessibilityLabel={label} onPress={onPress} style={styles.button}><Text style={styles.label}>{label}</Text></Pressable>
);

export default function App() {
  const [out, setOut] = useState('ready');
  const [watch, setWatch] = useState('');
  // `run` shows what a button's async work produced: its text, or the error message.
  const run = (f) => async () => { try { setOut(await f()); } catch (e) { setOut(`error: ${e.message}`); } };
  return (
    <View style={styles.container}>
      <Text style={styles.line} accessibilityLabel="Output">{`Output: ${out}`}</Text>
      <Text style={styles.line} accessibilityLabel="Watch output">{`Watch: ${watch}`}</Text>
      <Button label="Copy" onPress={run(async () => { await Clipboard.setStringAsync('hello'); return 'copied'; })} />
      <Button label="Paste" onPress={run(async () => `pasted ${await Clipboard.getStringAsync()}`)} />
      <Button label="Buzz" onPress={run(async () => { await Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium); return 'buzzed'; })} />
      <Button label="Locate" onPress={run(async () => {
        const p = await Location.requestForegroundPermissionsAsync();
        if (!p.granted) return `permission ${p.status}`;
        return `permission ${p.status} ${fmt(await Location.getCurrentPositionAsync({}))}`;
      })} />
      <Button label="Watch" onPress={run(async () => {
        await Location.watchPositionAsync({}, (l) => setWatch(fmt(l)));
        return 'watching';
      })} />
      <Button label="Write file" onPress={run(async () => {
        const f = new File(Paths.document, 'note.txt');
        f.create({ overwrite: true });
        f.write('hi');
        return `file ${await f.text()}`;
      })} />
      <Button label="List docs" onPress={run(async () => `docs ${new Directory(Paths.document).list().map((e) => e.name).join(',')}`)} />
      <Button label="Save secret" onPress={run(async () => { await SecureStore.setItemAsync('token', 'abc'); return 'saved'; })} />
      <Button label="Read secret" onPress={run(async () => `secret ${await SecureStore.getItemAsync('token')}`)} />
      <Button label="Digest" onPress={run(async () => `digest ${await Crypto.digestStringAsync(Crypto.CryptoDigestAlgorithm.SHA256, 'hello')}`)} />
      <Button label="Locale" onPress={run(async () => { const l = Localization.getLocales()[0], c = Localization.getCalendars()[0]; return `locale ${l.languageTag} ${l.regionCode} ${l.currencyCode} ${l.textDirection} ${c.calendar} ${c.timeZone}`; })} />
      <Button label="App info" onPress={run(async () => `app ${Application.applicationName} ${Application.nativeApplicationVersion} ${await Application.getIosIdForVendorAsync()}`)} />
      <Button label="Network" onPress={run(async () => { const n = await Network.getNetworkStateAsync(); return `net ${n.type} ${n.isConnected} ${await Network.getIpAddressAsync()}`; })} />
      <Button label="Battery" onPress={run(async () => { await Battery.addBatteryLevelListener(({ batteryLevel }) => setOut(`battery event ${batteryLevel}`)); return `battery ${await Battery.getBatteryLevelAsync()} ${await Battery.getBatteryStateAsync()} ${await Battery.isLowPowerModeEnabledAsync()}`; })} />
      <Button label="Brightness" onPress={run(async () => { await Brightness.setBrightnessAsync(0.8); return `brightness ${await Brightness.getBrightnessAsync()}`; })} />
      <Button label="Landscape" onPress={run(async () => { await ScreenOrientation.lockAsync(ScreenOrientation.OrientationLock.PORTRAIT_UP); await ScreenOrientation.lockAsync(ScreenOrientation.OrientationLock.LANDSCAPE_LEFT); return 'locked'; })} />
      <Button label="Review" onPress={run(async () => { await StoreReview.requestReview(); return `review ${await StoreReview.isAvailableAsync()}`; })} />
      <Button label="Track" onPress={run(async () => `track ${(await Tracking.requestTrackingPermissionsAsync()).status} ${Tracking.getAdvertisingId()}`)} />
      <Button label="Auth" onPress={run(async () => { const r = await LocalAuthentication.authenticateAsync({ promptMessage: 'Unlock' }); return `auth ${r.success}${r.error ? ' ' + r.error : ''}`; })} />
      <Button label="Vibrate" onPress={run(async () => { Vibration.vibrate(); return 'vibrated'; })} />
      <Button label="Store item" onPress={run(async () => { await AsyncStorage.setItem('k', JSON.stringify({ a: { b: 1 } })); await AsyncStorage.mergeItem('k', JSON.stringify({ a: { c: 2 } })); return 'stored'; })} />
      <Button label="Read item" onPress={run(async () => `item ${await AsyncStorage.getItem('k')} keys ${(await AsyncStorage.getAllKeys()).join(',')}`)} />
      <Button label="Camera" onPress={run(async () => `camera ${JSON.stringify(await Camera.requestCameraPermissionsAsync())}`)} />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, flexDirection: 'row', flexWrap: 'wrap', alignContent: 'center', justifyContent: 'center', gap: 6, backgroundColor: '#fff' },
  button: { backgroundColor: '#007AFF', paddingVertical: 6, paddingHorizontal: 10, borderRadius: 10 },
  line: { width: '100%', textAlign: 'center' },
  label: { color: '#fff', fontSize: 14 },
});
