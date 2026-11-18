// react-native-gesture-handler in the SimCore guest (issue #10). Driven by frameworks/react-native/guest/run-gestures.mjs.
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { Gesture, GestureDetector, GestureHandlerRootView, PanGestureHandler, Pressable, RectButton, State, TapGestureHandler } from 'react-native-gesture-handler';
import ReanimatedSwipeable from 'react-native-gesture-handler/ReanimatedSwipeable';
import Animated, { useAnimatedStyle, useSharedValue } from 'react-native-reanimated';

export default function App() {
  const [taps, setTaps] = useState(0);
  const [pan, setPan] = useState('idle');
  const [long, setLong] = useState('no');
  const [legacy, setLegacy] = useState('none');
  const [buttons, setButtons] = useState('none');
  const [swipe, setSwipe] = useState('closed');
  const x = useSharedValue(0);
  const moved = useAnimatedStyle(() => ({ transform: [{ translateX: x.value }] }));

  const tap = Gesture.Tap().runOnJS(true).onEnd(() => setTaps((n) => n + 1));
  const jsPan = Gesture.Pan().runOnJS(true)
    .onBegin(() => setPan('began'))
    .onUpdate((e) => setPan(`${Math.round(e.translationX)},${Math.round(e.translationY)}`))
    .onEnd((e) => setPan(`end ${Math.round(e.translationX)},${Math.round(e.translationY)}`));
  const uiPan = Gesture.Pan().onUpdate((e) => { x.value = e.translationX; }); // a Reanimated worklet
  const press = Gesture.LongPress().runOnJS(true).onStart(() => setLong('yes'));

  return (
    <GestureHandlerRootView style={s.root}>
      <GestureDetector gesture={tap}><View testID="tap" style={s.box}><Text>Taps: {taps}</Text></View></GestureDetector>
      <GestureDetector gesture={jsPan}><View testID="pan" style={s.box}><Text>Pan: {pan}</Text></View></GestureDetector>
      <GestureDetector gesture={uiPan}><Animated.View testID="uipan" style={[s.box, moved]}><Text>Drag me</Text></Animated.View></GestureDetector>
      <GestureDetector gesture={press}><View testID="long" style={s.box}><Text>Long: {long}</Text></View></GestureDetector>
      <TapGestureHandler onHandlerStateChange={(e) => e.nativeEvent.state === State.END && setLegacy('tapped')}>
        <View testID="legacytap" style={s.box}><Text>Legacy: {legacy}</Text></View>
      </TapGestureHandler>
      <PanGestureHandler onGestureEvent={(e) => setLegacy(`pan ${Math.round(e.nativeEvent.translationX)}`)}>
        <View testID="legacypan" style={s.box}><Text>Legacy pan</Text></View>
      </PanGestureHandler>
      <RectButton testID="rect" style={s.box} onPress={() => setButtons('rect')}><Text>Rect</Text></RectButton>
      <Pressable testID="ghpress" style={s.box} onPress={() => setButtons('pressable')}><Text>GH Pressable</Text></Pressable>
      <Text>Buttons: {buttons}</Text>
      <ReanimatedSwipeable testID="swipe" onSwipeableOpen={(d) => setSwipe(`open ${d}`)} onSwipeableClose={() => setSwipe('closed')}
        renderRightActions={() => <View style={s.action}><Text>Delete</Text></View>}>
        <View testID="row" style={s.row}><Text>Swipe: {swipe}</Text></View>
      </ReanimatedSwipeable>
    </GestureHandlerRootView>
  );
}

const s = StyleSheet.create({
  root: { flex: 1, paddingTop: 60 },
  box: { height: 50, margin: 4, backgroundColor: '#dde', justifyContent: 'center', paddingHorizontal: 12 },
  row: { height: 50, backgroundColor: '#fff', justifyContent: 'center', paddingHorizontal: 12 },
  action: { width: 80, backgroundColor: 'red', justifyContent: 'center', alignItems: 'center' },
});
