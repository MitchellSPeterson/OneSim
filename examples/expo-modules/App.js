import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import * as Haptics from 'expo-haptics';
import * as Location from 'expo-location';
import { Camera } from 'expo-camera';
import { Directory, File, Paths } from 'expo-file-system';
import * as SecureStore from 'expo-secure-store';

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
      <Text accessibilityLabel="Output">{`Output: ${out}`}</Text>
      <Text accessibilityLabel="Watch output">{`Watch: ${watch}`}</Text>
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
      <Button label="Camera" onPress={run(async () => `camera ${JSON.stringify(await Camera.requestCameraPermissionsAsync())}`)} />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12, backgroundColor: '#fff' },
  button: { backgroundColor: '#007AFF', paddingVertical: 10, paddingHorizontal: 20, borderRadius: 10 },
  label: { color: '#fff', fontSize: 17 },
});
