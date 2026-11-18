import { useEffect, useRef, useState } from 'react';
import { FlatList, Image, Keyboard, Modal, NativeModules, Pressable, ScrollView, Switch, Text, TextInput, View } from 'react-native';

const NET_IMG = 'http://127.0.0.1:8097/net.png';
export default function App() {
  const [text, setText] = useState('');
  const [up, setUp] = useState('');
  const [sub, setSub] = useState('');
  const [foc, setFoc] = useState('none');
  const [y, setY] = useState(-1);
  const [fy, setFy] = useState(-1);
  const [fl, setFl] = useState('');
  const [on, setOn] = useState(false);
  const [net, setNet] = useState('pending');
  const [post, setPost] = useState('pending');
  const [xhr, setXhr] = useState('pending');
  const [sizes, setSizes] = useState('pending');
  const [l1, setL1] = useState('');
  const [l2, setL2] = useState('');
  const [shown, setShown] = useState('no');
  const [kb, setKb] = useState('');
  const ref = useRef(null);
  const sref = useRef(null);
  useEffect(() => {
    const base = new URL(NativeModules.SourceCode.getConstants().scriptURL).origin;
    fetch(base + '/status').then((r) => r.text()).then(setNet, (e) => setNet('ERR ' + e.message));
    fetch('http://127.0.0.1:8097/echo', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ a: 1 }) })
      .then((r) => r.json()).then((j) => setPost(JSON.stringify(j)), (e) => setPost('ERR ' + e.message));
    const x = new XMLHttpRequest();
    x.open('GET', 'http://127.0.0.1:8097/hello');
    x.onload = () => setXhr(x.status + ':' + x.responseText);
    x.onerror = () => setXhr('ERR');
    x.send();
    Image.getSize(NET_IMG, (w, h) => setSizes(w + 'x' + h), (e) => setSizes('ERR'));
    const subs = ['keyboardWillShow', 'keyboardDidShow', 'keyboardWillHide', 'keyboardDidHide'].map((n) => Keyboard.addListener(n, (e) => {
      const ev = `${n.slice(8)}:${e.endCoordinates.height}@${e.endCoordinates.screenY}:${Keyboard.isVisible()},`;
      setKb((s) => s + ev);
    }));
    setTimeout(() => ref.current && ref.current.focus(), 800);
    setTimeout(() => sref.current && sref.current.scrollTo({ y: 50, animated: false }), 900);
    return () => subs.forEach((x) => x.remove());
  }, []);
  return (
    <View style={{ flex: 1, backgroundColor: '#fff' }}>
      <Text>{'text:' + text}</Text>
      <Text>{'up:' + up}</Text>
      <Text>{'sub:' + sub}</Text>
      <Text>{'foc:' + foc}</Text>
      <Text>{'y:' + y}</Text>
      <Text>{'fy:' + fy}</Text>
      <Text>{'fl:' + fl}</Text>
      <Text>{'sw:' + on}</Text>
      <Text>{'net:' + net}</Text>
      <Text>{'post:' + post}</Text>
      <Text>{'xhr:' + xhr}</Text>
      <Text>{'sizes:' + sizes}</Text>
      <Text>{'l1:' + l1}</Text>
      <Text>{'l2:' + l2}</Text>
      <Text>{'shown:' + shown}</Text>
      <Text>{'kb:' + kb}</Text>
      <Pressable onPress={() => Keyboard.dismiss()}><Text>dismiss</Text></Pressable>
      <Modal visible transparent onShow={() => setShown('yes')}><View /></Modal>
      <TextInput ref={ref} value={text} onChangeText={setText} onSubmitEditing={(e) => setSub(e.nativeEvent.text)}
        onFocus={() => setFoc('focus')} onBlur={() => setFoc('blur')} placeholder="type" style={{ height: 40 }} />
      <TextInput value={up} onChangeText={(t) => setUp(t.toUpperCase())} style={{ height: 40 }} />
      <TextInput defaultValue="abc" style={{ height: 40 }} />
      <Switch value={on} onValueChange={setOn} />
      <Image source={require('./assets/x.png')} style={{ width: 20, height: 10 }} onLoad={(e) => setL1(JSON.stringify(e.nativeEvent.source.width))} />
      <Image source={{ uri: NET_IMG }} style={{ width: 7, height: 5 }} onLoad={(e) => setL2(e.nativeEvent.source.width + 'x' + e.nativeEvent.source.height)} />
      <ScrollView ref={sref} style={{ height: 200 }} onScroll={(e) => setY(e.nativeEvent.contentOffset.y)} scrollEventThrottle={16}>
        <View style={{ height: 1000 }} />
      </ScrollView>
      <FlatList style={{ height: 200 }} data={Array.from({ length: 50 }, (_, i) => i)} keyExtractor={(i) => String(i)}
        renderItem={({ item }) => <Text>{'item' + item}</Text>}
        onLayout={(e) => setFl(e.nativeEvent.layout.width + 'x' + e.nativeEvent.layout.height)}
        onScroll={(e) => setFy(e.nativeEvent.contentOffset.y)} scrollEventThrottle={16} />
    </View>
  );
}
