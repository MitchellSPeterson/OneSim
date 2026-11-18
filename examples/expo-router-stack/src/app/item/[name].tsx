import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { Pressable, StyleSheet, Text, View } from 'react-native';

export default function Item() {
  const { name } = useLocalSearchParams<{ name: string }>();
  const router = useRouter();
  return (
    <View style={styles.page}>
      <Stack.Screen options={{ title: name }} />
      <Text style={styles.title}>You picked {name}</Text>
      <Pressable accessibilityLabel="Go back" onPress={() => router.back()} style={styles.button}>
        <Text style={styles.buttonText}>Go back</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, backgroundColor: '#fff', alignItems: 'center', justifyContent: 'center', gap: 20 },
  title: { fontSize: 24, fontWeight: '600', color: '#000' },
  button: { backgroundColor: '#007AFF', paddingHorizontal: 20, paddingVertical: 10, borderRadius: 10 },
  buttonText: { color: '#fff', fontSize: 17, fontWeight: '600' },
});
