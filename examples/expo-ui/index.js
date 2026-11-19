import { Button, Column, Host, RNHostView, Row, ScrollView, Slider, Switch, Text, TextInput } from '@expo/ui';
import { width } from '@expo/ui/jetpack-compose/modifiers';
import { frame } from '@expo/ui/swift-ui/modifiers';
import { registerRootComponent } from 'expo';
import { StatusBar } from 'expo-status-bar';
import { useState } from 'react';
import { Platform, Pressable, Text as RNText, View } from 'react-native';

function App() {
  const [count, setCount] = useState(0);
  const [enabled, setEnabled] = useState(false);
  const [volume, setVolume] = useState(25);
  const [name, setName] = useState('');
  const [submitted, setSubmitted] = useState('');
  const [embedded, setEmbedded] = useState(0);
  return (
    <View style={{ flex: 1, backgroundColor: '#fff', padding: 24, paddingTop: 104, gap: 12 }}>
      <StatusBar style="dark" />
      <RNText>React Native</RNText>
      <Host matchContents style={{ alignSelf: 'flex-start' }}>
        <Text>Expo UI</Text>
      </Host>
      <Host matchContents={{ vertical: true }} style={{ width: 220 }}>
        <Column spacing={12} alignment="start">
          <Row spacing={14} alignment="center">
            <Button label="Increment" onPress={() => setCount((value) => value + 1)} />
            <Text>Count {count}</Text>
          </Row>
          <Switch value={enabled} onValueChange={setEnabled} label="Notifications" />
          <Text>{enabled ? 'Notifications on' : 'Notifications off'}</Text>
          <TextInput
            placeholder="Name"
            onChangeText={setName}
            onSubmitEditing={setSubmitted}
            style={{ width: 200, padding: 8, backgroundColor: '#f1f0f5' }}
          />
          <Text>{submitted ? `Saved ${submitted}` : `Name: ${name}`}</Text>
          <Text>Volume {volume}</Text>
          <Slider value={volume} min={0} max={100} step={25} onValueChange={setVolume}
            modifiers={Platform.OS === 'ios' ? [frame({ width: 220 })] : [width(220)]} />
          <ScrollView style={{ width: 220, height: 100 }} showsIndicators={false}>
            {['First', 'Second', 'Third', 'Fourth', 'Fifth'].map((label) => (
              <Text key={label} style={{ height: 38 }}>{label}</Text>
            ))}
            <RNHostView matchContents>
              <Pressable accessibilityRole="button" onPress={() => setEmbedded((value) => value + 1)}
                style={{ padding: 8, backgroundColor: '#e0eafa' }}>
                <RNText>Embedded {embedded}</RNText>
              </Pressable>
            </RNHostView>
          </ScrollView>
        </Column>
      </Host>
      <RNText>Below the host</RNText>
    </View>
  );
}

registerRootComponent(App);
