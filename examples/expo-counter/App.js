import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

export default function App() {
  const [count, setCount] = useState(0);
  return (
    <View style={styles.container}>
      <Text style={styles.title}>Count: {count}</Text>
      <Pressable
        style={({ pressed }) => [styles.button, pressed && styles.pressed]}
        onPress={() => setCount((c) => c + 1)}
      >
        <Text style={styles.label}>Increment</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#fff', alignItems: 'center', justifyContent: 'center' },
  title: { fontSize: 32, fontWeight: '600', marginBottom: 24, color: '#111' },
  button: { backgroundColor: '#007AFF', paddingHorizontal: 24, paddingVertical: 12, borderRadius: 12 },
  pressed: { opacity: 0.6 },
  label: { color: '#fff', fontSize: 17, fontWeight: '600' },
});
