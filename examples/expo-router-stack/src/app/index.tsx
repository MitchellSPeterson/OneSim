import { Link } from 'expo-router';
import { Pressable, ScrollView, StyleSheet, Text } from 'react-native';

const FRUITS = ['Apple', 'Banana', 'Cherry', 'Mango', 'Peach'];

export default function Index() {
  return (
    <ScrollView contentInsetAdjustmentBehavior="automatic" style={styles.list}>
      {FRUITS.map((name) => (
        <Link key={name} href={{ pathname: '/item/[name]', params: { name } }} asChild>
          <Pressable accessibilityLabel={name} style={styles.row}>
            <Text style={styles.label}>{name}</Text>
          </Pressable>
        </Link>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  list: { flex: 1, backgroundColor: '#fff' },
  row: { paddingHorizontal: 20, paddingVertical: 14, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: '#C6C6C8' },
  label: { fontSize: 17, color: '#000' },
});
