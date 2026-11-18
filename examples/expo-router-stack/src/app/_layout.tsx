import { Stack } from 'expo-router';

export default function Layout() {
  return (
    <Stack>
      <Stack.Screen name="index" options={{ title: 'Fruits', headerLargeTitle: true }} />
      <Stack.Screen name="item/[name]" options={{ title: 'Detail' }} />
    </Stack>
  );
}
