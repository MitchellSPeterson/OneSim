import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Animated,
  FlatList,
  Modal,
  Platform,
  Pressable,
  RefreshControl,
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  useColorScheme,
  View,
} from 'react-native';
import { BlurView } from 'expo-blur';
import { LinearGradient } from 'expo-linear-gradient';
import { StatusBar } from 'expo-status-bar';

const PALETTES = [
  {
    id: 'coral',
    name: 'Coral',
    primary: '#FF5E62',
    secondary: '#FF9966',
    accent: '#FFD166',
    mood: 'Sunset Energy',
  },
  {
    id: 'mint',
    name: 'Mint',
    primary: '#00B894',
    secondary: '#00CEC9',
    accent: '#55EFC4',
    mood: 'Fresh Focus',
  },
  {
    id: 'berry',
    name: 'Berry',
    primary: '#8E44AD',
    secondary: '#E84393',
    accent: '#FD79A8',
    mood: 'Electric Flow',
  },
  {
    id: 'solar',
    name: 'Solar',
    primary: '#F39C12',
    secondary: '#E67E22',
    accent: '#F1C40F',
    mood: 'Golden Spark',
  },
];

const CATEGORIES = ['Play', 'Make', 'Move', 'Calm'];

const CATEGORY_COLORS = {
  Play: '#FF5E62',
  Make: '#8E44AD',
  Move: '#00B894',
  Calm: '#F39C12',
};

const INITIAL_SPARKS = [
  { id: '1', title: 'Sketch a cloud from memory', category: 'Make', done: true },
  { id: '2', title: 'Capture a sunset color swatch', category: 'Play', done: false },
  { id: '3', title: 'Ship a tiny UI delight', category: 'Make', done: false },
  { id: '4', title: 'Hydrate & stretch for 2 minutes', category: 'Move', done: true },
  { id: '5', title: 'Take 5 slow prism breaths', category: 'Calm', done: false },
];

const RADIUS_PRESETS = [8, 16, 24, 34];
const BLUR_PRESETS = [20, 45, 70, 95];
const SPEED_PRESETS = [
  { id: 'calm', label: 'Calm', duration: 2400 },
  { id: 'flow', label: 'Flow', duration: 1400 },
  { id: 'hyper', label: 'Hyper', duration: 700 },
];

const TABS = [
  { id: 'pulse', label: 'Pulse', glyph: '◈' },
  { id: 'sparks', label: 'Sparks', glyph: '✦' },
  { id: 'lab', label: 'Lab', glyph: '◎' },
];

export default function App() {
  const colorScheme = useColorScheme();
  const isDark = colorScheme === 'dark';
  const isIOS = Platform.OS === 'ios';

  const [activeTab, setActiveTab] = useState('pulse');
  const [paletteId, setPaletteId] = useState('coral');

  // Pulse state
  const [vibeScore, setVibeScore] = useState(42);
  const [energyLevel, setEnergyLevel] = useState(64);
  const [autoPulse, setAutoPulse] = useState(true);
  const [glowMode, setGlowMode] = useState(true);
  const [pulseRefreshing, setPulseRefreshing] = useState(false);

  // Sparks state
  const [sparks, setSparks] = useState(INITIAL_SPARKS);
  const [sparksRefreshing, setSparksRefreshing] = useState(false);
  const [composerOpen, setComposerOpen] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const [newCategory, setNewCategory] = useState('Play');
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);

  // Lab state
  const [labRadius, setLabRadius] = useState(24);
  const [labBlur, setLabBlur] = useState(45);
  const [labSpeedId, setLabSpeedId] = useState('flow');
  const [labBlurEnabled, setLabBlurEnabled] = useState(true);
  const [labAutoPulse, setLabAutoPulse] = useState(true);
  const [labSpinning, setLabSpinning] = useState(true);
  const [focusOpen, setFocusOpen] = useState(false);

  const palette = useMemo(
    () => PALETTES.find((p) => p.id === paletteId) || PALETTES[0],
    [paletteId]
  );

  const labSpeed = useMemo(
    () => SPEED_PRESETS.find((s) => s.id === labSpeedId) || SPEED_PRESETS[1],
    [labSpeedId]
  );

  const theme = useMemo(
    () => ({
      bg: isDark ? '#0D0E14' : '#F6F7FB',
      card: isDark ? '#171923' : '#FFFFFF',
      cardAlt: isDark ? '#1F2230' : '#EEF0F7',
      border: isDark ? '#2A2E40' : '#E2E6F0',
      text: isDark ? '#F5F7FF' : '#161824',
      textMuted: isDark ? '#9DA3B8' : '#636980',
      tabBar: isDark ? '#141620' : '#FFFFFF',
      blurTint: isDark ? 'dark' : 'light',
    }),
    [isDark]
  );

  // Animated values (all useNativeDriver: false for OneSim compatibility)
  const orbOneY = useRef(new Animated.Value(0)).current;
  const orbOneOpacity = useRef(new Animated.Value(0.75)).current;
  const orbTwoY = useRef(new Animated.Value(0)).current;
  const orbTwoX = useRef(new Animated.Value(0)).current;
  const energyAnim = useRef(new Animated.Value(64)).current;
  const sparkProgressAnim = useRef(new Animated.Value(40)).current;
  const labOrbX = useRef(new Animated.Value(0)).current;
  const labOrbOpacity = useRef(new Animated.Value(0.85)).current;
  const focusOrbY = useRef(new Animated.Value(0)).current;
  const focusOrbOpacity = useRef(new Animated.Value(0.6)).current;

  // Pulse hero orb loop
  useEffect(() => {
    if (!autoPulse) {
      orbOneY.setValue(0);
      orbOneOpacity.setValue(0.75);
      orbTwoY.setValue(0);
      orbTwoX.setValue(0);
      return;
    }

    const loopOne = Animated.loop(
      Animated.sequence([
        Animated.parallel([
          Animated.timing(orbOneY, {
            toValue: -12,
            duration: 1400,
            useNativeDriver: false,
          }),
          Animated.timing(orbOneOpacity, {
            toValue: 1,
            duration: 1400,
            useNativeDriver: false,
          }),
        ]),
        Animated.parallel([
          Animated.timing(orbOneY, {
            toValue: 0,
            duration: 1400,
            useNativeDriver: false,
          }),
          Animated.timing(orbOneOpacity, {
            toValue: 0.6,
            duration: 1400,
            useNativeDriver: false,
          }),
        ]),
      ])
    );

    const loopTwo = Animated.loop(
      Animated.sequence([
        Animated.parallel([
          Animated.timing(orbTwoY, {
            toValue: 10,
            duration: 1800,
            useNativeDriver: false,
          }),
          Animated.timing(orbTwoX, {
            toValue: 10,
            duration: 1800,
            useNativeDriver: false,
          }),
        ]),
        Animated.parallel([
          Animated.timing(orbTwoY, {
            toValue: 0,
            duration: 1800,
            useNativeDriver: false,
          }),
          Animated.timing(orbTwoX, {
            toValue: 0,
            duration: 1800,
            useNativeDriver: false,
          }),
        ]),
      ])
    );

    loopOne.start();
    loopTwo.start();
    return () => {
      loopOne.stop();
      loopTwo.stop();
    };
  }, [autoPulse, orbOneOpacity, orbOneY, orbTwoX, orbTwoY]);

  // Animate energy bar width
  useEffect(() => {
    Animated.timing(energyAnim, {
      toValue: energyLevel,
      duration: 240,
      useNativeDriver: false,
    }).start();
  }, [energyAnim, energyLevel]);

  // Animate Sparks progress bar
  const completedCount = useMemo(
    () => sparks.filter((s) => s.done).length,
    [sparks]
  );
  const progressPct = useMemo(
    () => (sparks.length === 0 ? 0 : Math.round((completedCount / sparks.length) * 100)),
    [completedCount, sparks.length]
  );

  useEffect(() => {
    Animated.timing(sparkProgressAnim, {
      toValue: progressPct,
      duration: 260,
      useNativeDriver: false,
    }).start();
  }, [progressPct, sparkProgressAnim]);

  // Lab preview orb loop
  useEffect(() => {
    if (!labAutoPulse) {
      labOrbX.setValue(0);
      labOrbOpacity.setValue(0.85);
      return;
    }

    const loop = Animated.loop(
      Animated.sequence([
        Animated.parallel([
          Animated.timing(labOrbX, {
            toValue: 36,
            duration: labSpeed.duration,
            useNativeDriver: false,
          }),
          Animated.timing(labOrbOpacity, {
            toValue: 0.45,
            duration: labSpeed.duration,
            useNativeDriver: false,
          }),
        ]),
        Animated.parallel([
          Animated.timing(labOrbX, {
            toValue: -36,
            duration: labSpeed.duration,
            useNativeDriver: false,
          }),
          Animated.timing(labOrbOpacity, {
            toValue: 1,
            duration: labSpeed.duration,
            useNativeDriver: false,
          }),
        ]),
      ])
    );

    loop.start();
    return () => loop.stop();
  }, [labAutoPulse, labOrbOpacity, labOrbX, labSpeed.duration]);

  // Focus Mode breathing orb loop
  useEffect(() => {
    if (!focusOpen) {
      focusOrbY.setValue(0);
      focusOrbOpacity.setValue(0.6);
      return;
    }

    const loop = Animated.loop(
      Animated.sequence([
        Animated.parallel([
          Animated.timing(focusOrbY, {
            toValue: -26,
            duration: 2200,
            useNativeDriver: false,
          }),
          Animated.timing(focusOrbOpacity, {
            toValue: 1,
            duration: 2200,
            useNativeDriver: false,
          }),
        ]),
        Animated.parallel([
          Animated.timing(focusOrbY, {
            toValue: 14,
            duration: 2200,
            useNativeDriver: false,
          }),
          Animated.timing(focusOrbOpacity, {
            toValue: 0.45,
            duration: 2200,
            useNativeDriver: false,
          }),
        ]),
      ])
    );

    loop.start();
    return () => loop.stop();
  }, [focusOpen, focusOrbOpacity, focusOrbY]);

  const handleSurge = () => {
    setVibeScore((prev) => prev + 5);
    setEnergyLevel((prev) => (prev >= 100 ? 25 : Math.min(100, prev + 12)));
  };

  const handleResetPulse = () => {
    setVibeScore(42);
    setEnergyLevel(64);
  };

  const handlePulseRefresh = () => {
    setPulseRefreshing(true);
    setTimeout(() => {
      setVibeScore((prev) => prev + 1);
      setEnergyLevel(72);
      setPulseRefreshing(false);
    }, 500);
  };

  const handleSparksRefresh = () => {
    setSparksRefreshing(true);
    setTimeout(() => {
      setSparksRefreshing(false);
    }, 500);
  };

  const toggleSpark = (id) => {
    setSparks((prev) =>
      prev.map((item) => (item.id === id ? { ...item, done: !item.done } : item))
    );
  };

  const addSpark = () => {
    const trimmed = newTitle.trim();
    if (!trimmed) return;
    setSparks((prev) => [
      {
        id: String(Date.now()),
        title: trimmed,
        category: newCategory,
        done: false,
      },
      ...prev,
    ]);
    setNewTitle('');
    setNewCategory('Play');
    setComposerOpen(false);
  };

  const clearCompletedSparks = () => {
    setSparks((prev) => prev.filter((item) => !item.done));
    setConfirmClearOpen(false);
  };

  const requestClearDone = () => {
    if (completedCount === 0) return;
    if (isIOS) {
      Alert.alert(
        'Clear Completed Sparks',
        `Remove ${completedCount} completed spark${completedCount === 1 ? '' : 's'}?`,
        [
          { text: 'Cancel', style: 'cancel' },
          { text: 'Clear', style: 'destructive', onPress: clearCompletedSparks },
        ]
      );
    } else {
      setConfirmClearOpen(true);
    }
  };

  const energyWidth = energyAnim.interpolate({
    inputRange: [0, 100],
    outputRange: ['0%', '100%'],
  });

  const sparkProgressWidth = sparkProgressAnim.interpolate({
    inputRange: [0, 100],
    outputRange: ['0%', '100%'],
  });

  const renderPulseTab = () => (
    <ScrollView
      style={styles.flex}
      contentContainerStyle={styles.scrollContent}
      refreshControl={
        <RefreshControl
          refreshing={pulseRefreshing}
          onRefresh={handlePulseRefresh}
          tintColor={palette.primary}
          colors={[palette.primary]}
        />
      }
    >
      <View style={styles.headerRow}>
        <View>
          <Text style={[styles.eyebrow, { color: palette.primary }]}>PRISM STUDIO</Text>
          <Text style={[styles.screenTitle, { color: theme.text }]}>Pulse</Text>
        </View>
        <View style={[styles.modeBadge, { backgroundColor: theme.cardAlt, borderColor: theme.border }]}>
          <Text style={[styles.modeBadgeText, { color: theme.textMuted }]}>
            {isIOS ? 'iOS' : 'Android'} · {isDark ? 'Dark' : 'Light'}
          </Text>
        </View>
      </View>

      {/* Hero Gradient Card */}
      <LinearGradient
        colors={
          glowMode
            ? [palette.primary, palette.secondary, palette.accent]
            : [palette.primary, palette.secondary]
        }
        start={{ x: 0, y: 0 }}
        end={{ x: 1, y: 1 }}
        style={styles.heroCard}
      >
        <Animated.View
          style={[
            styles.heroOrbLarge,
            {
              backgroundColor: palette.accent,
              opacity: orbOneOpacity,
              transform: [{ translateY: orbOneY }],
            },
          ]}
        />
        <Animated.View
          style={[
            styles.heroOrbSmall,
            {
              backgroundColor: '#FFFFFF',
              opacity: glowMode ? 0.35 : 0.18,
              transform: [{ translateY: orbTwoY }, { translateX: orbTwoX }],
            },
          ]}
        />

        <View style={styles.heroTopRow}>
          <BlurView intensity={40} tint={theme.blurTint} style={styles.blurPill}>
            <Text style={styles.blurPillText}>
              {palette.name} · {palette.mood}
            </Text>
          </BlurView>
          <View style={styles.liveDotPill}>
            <View style={[styles.liveDot, { backgroundColor: palette.accent }]} />
            <Text style={styles.liveDotText}>{autoPulse ? 'LIVE' : 'STILL'}</Text>
          </View>
        </View>

        <Text style={styles.heroScoreLabel}>VIBE SCORE</Text>
        <Text style={styles.heroScoreValue}>{vibeScore}</Text>
        <Text style={styles.heroSubtitle}>
          Pull to refresh or tap Vibe Pad below to surge studio energy.
        </Text>
      </LinearGradient>

      {/* Palette Switcher */}
      <Text style={[styles.sectionLabel, { color: theme.textMuted }]}>PALETTE</Text>
      <View style={styles.paletteRow}>
        {PALETTES.map((p) => {
          const selected = p.id === palette.id;
          return (
            <Pressable
              key={p.id}
              onPress={() => setPaletteId(p.id)}
              style={[
                styles.palettePill,
                {
                  backgroundColor: selected ? p.primary : theme.card,
                  borderColor: selected ? p.primary : theme.border,
                },
              ]}
            >
              <View
                style={[
                  styles.paletteSwatch,
                  { backgroundColor: selected ? '#FFFFFF' : p.primary },
                ]}
              />
              <Text
                style={[
                  styles.palettePillText,
                  { color: selected ? '#FFFFFF' : theme.text },
                ]}
              >
                {p.name}
              </Text>
            </Pressable>
          );
        })}
      </View>

      {/* Interactive Vibe Pad */}
      <View style={[styles.card, { backgroundColor: theme.card, borderColor: theme.border }]}>
        <View style={styles.cardHeaderRow}>
          <View>
            <Text style={[styles.cardTitle, { color: theme.text }]}>Vibe Pad</Text>
            <Text style={[styles.cardHint, { color: theme.textMuted }]}>
              Energy meter · {energyLevel}%
            </Text>
          </View>
          <Pressable
            onPress={handleResetPulse}
            style={[styles.smallGhostBtn, { borderColor: theme.border }]}
          >
            <Text style={[styles.smallGhostBtnText, { color: theme.textMuted }]}>Reset</Text>
          </Pressable>
        </View>

        <View style={[styles.progressTrack, { backgroundColor: theme.cardAlt }]}>
          <Animated.View
            style={[
              styles.progressFill,
              { width: energyWidth, backgroundColor: palette.primary },
            ]}
          />
        </View>

        <Pressable onPress={handleSurge} style={styles.surgePressable}>
          <LinearGradient
            colors={[palette.primary, palette.secondary]}
            start={{ x: 0, y: 0 }}
            end={{ x: 1, y: 0 }}
            style={styles.surgeButton}
          >
            <Text style={styles.surgeButtonText}>✦ Tap to Surge Energy (+5)</Text>
          </LinearGradient>
        </Pressable>
      </View>

      {/* Quick Controls */}
      <View style={[styles.card, { backgroundColor: theme.card, borderColor: theme.border }]}>
        <View style={styles.switchRow}>
          <View style={styles.flex}>
            <Text style={[styles.switchTitle, { color: theme.text }]}>Auto-Pulse Orbs</Text>
            <Text style={[styles.switchSub, { color: theme.textMuted }]}>
              Continuous floating motion on hero orbs
            </Text>
          </View>
          <Switch
            value={autoPulse}
            onValueChange={setAutoPulse}
            trackColor={{ false: theme.border, true: palette.primary }}
            thumbColor="#FFFFFF"
          />
        </View>

        <View style={[styles.divider, { backgroundColor: theme.border }]} />

        <View style={styles.switchRow}>
          <View style={styles.flex}>
            <Text style={[styles.switchTitle, { color: theme.text }]}>Glow Mode</Text>
            <Text style={[styles.switchSub, { color: theme.textMuted }]}>
              Tri-stop accent gradient & luminous orbs
            </Text>
          </View>
          <Switch
            value={glowMode}
            onValueChange={setGlowMode}
            trackColor={{ false: theme.border, true: palette.secondary }}
            thumbColor="#FFFFFF"
          />
        </View>
      </View>
    </ScrollView>
  );

  const renderSparksHeader = () => (
    <View style={styles.sparksHeaderWrap}>
      <View style={styles.headerRow}>
        <View>
          <Text style={[styles.eyebrow, { color: palette.primary }]}>DAILY MICRO-QUESTS</Text>
          <Text style={[styles.screenTitle, { color: theme.text }]}>Sparks</Text>
        </View>
        <Pressable
          onPress={() => setComposerOpen(true)}
          style={[styles.primaryChipBtn, { backgroundColor: palette.primary }]}
        >
          <Text style={styles.primaryChipBtnText}>+ New Spark</Text>
        </Pressable>
      </View>

      <View style={[styles.card, { backgroundColor: theme.card, borderColor: theme.border }]}>
        <View style={styles.cardHeaderRow}>
          <Text style={[styles.cardTitle, { color: theme.text }]}>
            {completedCount} of {sparks.length} completed
          </Text>
          <Text style={[styles.percentText, { color: palette.primary }]}>{progressPct}%</Text>
        </View>
        <View style={[styles.progressTrack, { backgroundColor: theme.cardAlt }]}>
          <Animated.View
            style={[
              styles.progressFill,
              { width: sparkProgressWidth, backgroundColor: palette.primary },
            ]}
          />
        </View>
        <View style={styles.sparksActionRow}>
          <Text style={[styles.cardHint, { color: theme.textMuted }]}>
            Tap any spark card to toggle done
          </Text>
          <Pressable
            onPress={requestClearDone}
            disabled={completedCount === 0}
            style={[
              styles.clearBtn,
              {
                borderColor: completedCount > 0 ? palette.primary : theme.border,
                opacity: completedCount > 0 ? 1 : 0.45,
              },
            ]}
          >
            <Text
              style={[
                styles.clearBtnText,
                { color: completedCount > 0 ? palette.primary : theme.textMuted },
              ]}
            >
              Clear Done
            </Text>
          </Pressable>
        </View>
      </View>
    </View>
  );

  const renderSparkItem = ({ item }) => {
    const tagColor = CATEGORY_COLORS[item.category] || palette.primary;
    return (
      <Pressable
        onPress={() => toggleSpark(item.id)}
        style={[
          styles.sparkCard,
          {
            backgroundColor: theme.card,
            borderColor: item.done ? palette.primary : theme.border,
          },
        ]}
      >
        <View
          style={[
            styles.sparkCheck,
            {
              backgroundColor: item.done ? palette.primary : 'transparent',
              borderColor: item.done ? palette.primary : theme.textMuted,
            },
          ]}
        >
          {item.done ? <Text style={styles.sparkCheckMark}>✓</Text> : null}
        </View>
        <View style={styles.flex}>
          <Text
            style={[
              styles.sparkTitle,
              {
                color: item.done ? theme.textMuted : theme.text,
                textDecorationLine: item.done ? 'line-through' : 'none',
              },
            ]}
          >
            {item.title}
          </Text>
          <View style={styles.sparkMetaRow}>
            <View style={[styles.categoryBadge, { backgroundColor: tagColor }]}>
              <Text style={styles.categoryBadgeText}>{item.category}</Text>
            </View>
            <Text style={[styles.sparkStatusText, { color: theme.textMuted }]}>
              {item.done ? 'Completed' : 'Tap to complete'}
            </Text>
          </View>
        </View>
      </Pressable>
    );
  };

  const renderSparksTab = () => (
    <FlatList
      style={styles.flex}
      contentContainerStyle={styles.scrollContent}
      data={sparks}
      keyExtractor={(item) => item.id}
      ListHeaderComponent={renderSparksHeader}
      renderItem={renderSparkItem}
      ListEmptyComponent={
        <View style={[styles.card, { backgroundColor: theme.card, borderColor: theme.border }]}>
          <Text style={[styles.cardTitle, { color: theme.text }]}>All clear!</Text>
          <Text style={[styles.cardHint, { color: theme.textMuted, marginTop: 4 }]}>
            Tap "+ New Spark" to add a fresh creative prompt.
          </Text>
        </View>
      }
      refreshControl={
        <RefreshControl
          refreshing={sparksRefreshing}
          onRefresh={handleSparksRefresh}
          tintColor={palette.primary}
          colors={[palette.primary]}
        />
      }
    />
  );

  const renderLabTab = () => (
    <ScrollView style={styles.flex} contentContainerStyle={styles.scrollContent}>
      <View style={styles.headerRow}>
        <View>
          <Text style={[styles.eyebrow, { color: palette.primary }]}>INTERACTIVE EFFECTS</Text>
          <Text style={[styles.screenTitle, { color: theme.text }]}>Lab</Text>
        </View>
        <Pressable
          onPress={() => setFocusOpen(true)}
          style={[styles.primaryChipBtn, { backgroundColor: palette.primary }]}
        >
          <Text style={styles.primaryChipBtnText}>◎ Focus Mode</Text>
        </Pressable>
      </View>

      {/* Live Preview Card */}
      <LinearGradient
        colors={[palette.primary, palette.secondary, palette.accent]}
        start={{ x: 0, y: 0 }}
        end={{ x: 1, y: 1 }}
        style={[styles.labPreviewCard, { borderRadius: labRadius }]}
      >
        <Animated.View
          style={[
            styles.labFloatingOrb,
            {
              backgroundColor: '#FFFFFF',
              opacity: labOrbOpacity,
              transform: [{ translateX: labOrbX }],
            },
          ]}
        />
        {labBlurEnabled ? (
          <BlurView
            intensity={labBlur}
            tint={theme.blurTint}
            style={[styles.labBlurBox, { borderRadius: Math.max(8, labRadius - 8) }]}
          >
            <Text style={styles.labPreviewTitle}>Live Surface</Text>
            <Text style={styles.labPreviewMeta}>
              Radius {labRadius}px · Blur {labBlur} · {labSpeed.label}
            </Text>
            {labSpinning ? (
              <View style={styles.labSpinnerRow}>
                <ActivityIndicator size="small" color="#FFFFFF" />
                <Text style={styles.labSpinnerText}>Rendering live shader preview</Text>
              </View>
            ) : null}
          </BlurView>
        ) : (
          <View
            style={[
              styles.labSolidBox,
              { borderRadius: Math.max(8, labRadius - 8) },
            ]}
          >
            <Text style={styles.labPreviewTitle}>Live Surface (No Blur)</Text>
            <Text style={styles.labPreviewMeta}>
              Radius {labRadius}px · {labSpeed.label}
            </Text>
            {labSpinning ? (
              <View style={styles.labSpinnerRow}>
                <ActivityIndicator size="small" color="#FFFFFF" />
                <Text style={styles.labSpinnerText}>Rendering live preview</Text>
              </View>
            ) : null}
          </View>
        )}
      </LinearGradient>

      {/* Preset Controls */}
      <View style={[styles.card, { backgroundColor: theme.card, borderColor: theme.border }]}>
        <Text style={[styles.controlLabel, { color: theme.textMuted }]}>CORNER RADIUS</Text>
        <View style={styles.presetRow}>
          {RADIUS_PRESETS.map((r) => {
            const active = r === labRadius;
            return (
              <Pressable
                key={r}
                onPress={() => setLabRadius(r)}
                style={[
                  styles.presetPill,
                  {
                    backgroundColor: active ? palette.primary : theme.cardAlt,
                    borderColor: active ? palette.primary : theme.border,
                  },
                ]}
              >
                <Text style={[styles.presetPillText, { color: active ? '#FFFFFF' : theme.text }]}>
                  {r}px
                </Text>
              </Pressable>
            );
          })}
        </View>

        <Text style={[styles.controlLabel, { color: theme.textMuted, marginTop: 14 }]}>
          BLUR INTENSITY
        </Text>
        <View style={styles.presetRow}>
          {BLUR_PRESETS.map((b) => {
            const active = b === labBlur;
            return (
              <Pressable
                key={b}
                onPress={() => setLabBlur(b)}
                style={[
                  styles.presetPill,
                  {
                    backgroundColor: active ? palette.secondary : theme.cardAlt,
                    borderColor: active ? palette.secondary : theme.border,
                  },
                ]}
              >
                <Text style={[styles.presetPillText, { color: active ? '#FFFFFF' : theme.text }]}>
                  {b}
                </Text>
              </Pressable>
            );
          })}
        </View>

        <Text style={[styles.controlLabel, { color: theme.textMuted, marginTop: 14 }]}>
          PULSE SPEED
        </Text>
        <View style={styles.presetRow}>
          {SPEED_PRESETS.map((s) => {
            const active = s.id === labSpeedId;
            return (
              <Pressable
                key={s.id}
                onPress={() => setLabSpeedId(s.id)}
                style={[
                  styles.presetPill,
                  {
                    backgroundColor: active ? palette.primary : theme.cardAlt,
                    borderColor: active ? palette.primary : theme.border,
                  },
                ]}
              >
                <Text style={[styles.presetPillText, { color: active ? '#FFFFFF' : theme.text }]}>
                  {s.label}
                </Text>
              </Pressable>
            );
          })}
        </View>
      </View>

      {/* Lab Toggles */}
      <View style={[styles.card, { backgroundColor: theme.card, borderColor: theme.border }]}>
        <View style={styles.switchRow}>
          <Text style={[styles.switchTitle, { color: theme.text }]}>BlurView Glass Layer</Text>
          <Switch
            value={labBlurEnabled}
            onValueChange={setLabBlurEnabled}
            trackColor={{ false: theme.border, true: palette.primary }}
            thumbColor="#FFFFFF"
          />
        </View>
        <View style={[styles.divider, { backgroundColor: theme.border }]} />
        <View style={styles.switchRow}>
          <Text style={[styles.switchTitle, { color: theme.text }]}>Auto-Pulse Motion</Text>
          <Switch
            value={labAutoPulse}
            onValueChange={setLabAutoPulse}
            trackColor={{ false: theme.border, true: palette.secondary }}
            thumbColor="#FFFFFF"
          />
        </View>
        <View style={[styles.divider, { backgroundColor: theme.border }]} />
        <View style={styles.switchRow}>
          <Text style={[styles.switchTitle, { color: theme.text }]}>Activity Indicator</Text>
          <Switch
            value={labSpinning}
            onValueChange={setLabSpinning}
            trackColor={{ false: theme.border, true: palette.primary }}
            thumbColor="#FFFFFF"
          />
        </View>
      </View>

      {/* Platform Status Card */}
      <View style={[styles.card, { backgroundColor: theme.card, borderColor: theme.border }]}>
        <View style={styles.cardHeaderRow}>
          <Text style={[styles.cardTitle, { color: theme.text }]}>Platform Status</Text>
          <View style={[styles.platformBadge, { backgroundColor: palette.primary }]}>
            <Text style={styles.platformBadgeText}>{isIOS ? 'iOS' : 'Android'}</Text>
          </View>
        </View>
        <Text style={[styles.cardHint, { color: theme.textMuted, marginTop: 6 }]}>
          Appearance: {isDark ? 'Dark' : 'Light'} · Dialogs:{' '}
          {isIOS ? 'Native Alert.alert' : 'Custom Modal Confirm'}
        </Text>
        <Text style={[styles.cardHint, { color: theme.textMuted, marginTop: 4 }]}>
          Active capabilities: LinearGradient, BlurView, Animated, ScrollView, FlatList,
          RefreshControl, Switch, TextInput, Modal, ActivityIndicator.
        </Text>
      </View>
    </ScrollView>
  );

  return (
    <SafeAreaView style={[styles.root, { backgroundColor: theme.bg }]}>
      <StatusBar style={isDark ? 'light' : 'dark'} />

      <View style={styles.flex}>
        {activeTab === 'pulse' && renderPulseTab()}
        {activeTab === 'sparks' && renderSparksTab()}
        {activeTab === 'lab' && renderLabTab()}
      </View>

      {/* Colorful Bottom Tab Bar */}
      <View
        style={[
          styles.tabBar,
          { backgroundColor: theme.tabBar, borderTopColor: theme.border },
        ]}
      >
        {TABS.map((tab) => {
          const active = tab.id === activeTab;
          return (
            <Pressable
              key={tab.id}
              onPress={() => setActiveTab(tab.id)}
              style={[
                styles.tabItem,
                active && { backgroundColor: palette.primary },
              ]}
            >
              <Text
                style={[
                  styles.tabGlyph,
                  { color: active ? '#FFFFFF' : theme.textMuted },
                ]}
              >
                {tab.glyph}
              </Text>
              <Text
                style={[
                  styles.tabLabel,
                  { color: active ? '#FFFFFF' : theme.textMuted },
                ]}
              >
                {tab.label}
              </Text>
            </Pressable>
          );
        })}
      </View>

      {/* New Spark Composer Modal */}
      <Modal
        visible={composerOpen}
        transparent
        animationType="slide"
        onRequestClose={() => setComposerOpen(false)}
      >
        <View style={styles.modalBackdrop}>
          <View
            style={[
              styles.modalSheet,
              { backgroundColor: theme.card, borderColor: theme.border },
            ]}
          >
            <LinearGradient
              colors={[palette.primary, palette.secondary]}
              start={{ x: 0, y: 0 }}
              end={{ x: 1, y: 0 }}
              style={styles.modalAccentBar}
            />
            <Text style={[styles.modalTitle, { color: theme.text }]}>New Spark</Text>
            <Text style={[styles.cardHint, { color: theme.textMuted, marginBottom: 12 }]}>
              Capture a quick creative micro-quest
            </Text>

            <TextInput
              value={newTitle}
              onChangeText={setNewTitle}
              placeholder="e.g., Paint a 3-color gradient"
              placeholderTextColor={theme.textMuted}
              style={[
                styles.textInput,
                {
                  backgroundColor: theme.cardAlt,
                  color: theme.text,
                  borderColor: theme.border,
                },
              ]}
            />

            <Text style={[styles.controlLabel, { color: theme.textMuted, marginTop: 14 }]}>
              CATEGORY
            </Text>
            <View style={styles.presetRow}>
              {CATEGORIES.map((cat) => {
                const selected = cat === newCategory;
                const catColor = CATEGORY_COLORS[cat];
                return (
                  <Pressable
                    key={cat}
                    onPress={() => setNewCategory(cat)}
                    style={[
                      styles.presetPill,
                      {
                        backgroundColor: selected ? catColor : theme.cardAlt,
                        borderColor: selected ? catColor : theme.border,
                      },
                    ]}
                  >
                    <Text
                      style={[
                        styles.presetPillText,
                        { color: selected ? '#FFFFFF' : theme.text },
                      ]}
                    >
                      {cat}
                    </Text>
                  </Pressable>
                );
              })}
            </View>

            <View style={styles.modalActions}>
              <Pressable
                onPress={() => setComposerOpen(false)}
                style={[styles.modalBtnSecondary, { borderColor: theme.border }]}
              >
                <Text style={[styles.modalBtnSecondaryText, { color: theme.text }]}>Cancel</Text>
              </Pressable>
              <Pressable
                onPress={addSpark}
                style={[
                  styles.modalBtnPrimary,
                  {
                    backgroundColor: palette.primary,
                    opacity: newTitle.trim().length > 0 ? 1 : 0.5,
                  },
                ]}
              >
                <Text style={styles.modalBtnPrimaryText}>Add Spark</Text>
              </Pressable>
            </View>
          </View>
        </View>
      </Modal>

      {/* Android Confirmation Modal for Clear Done */}
      <Modal
        visible={confirmClearOpen}
        transparent
        animationType="fade"
        onRequestClose={() => setConfirmClearOpen(false)}
      >
        <View style={styles.modalBackdrop}>
          <View
            style={[
              styles.modalSheet,
              { backgroundColor: theme.card, borderColor: theme.border },
            ]}
          >
            <Text style={[styles.modalTitle, { color: theme.text }]}>Clear Completed Sparks?</Text>
            <Text style={[styles.cardHint, { color: theme.textMuted, marginTop: 6 }]}>
              This removes {completedCount} completed spark{completedCount === 1 ? '' : 's'} from
              your list.
            </Text>
            <View style={styles.modalActions}>
              <Pressable
                onPress={() => setConfirmClearOpen(false)}
                style={[styles.modalBtnSecondary, { borderColor: theme.border }]}
              >
                <Text style={[styles.modalBtnSecondaryText, { color: theme.text }]}>Cancel</Text>
              </Pressable>
              <Pressable
                onPress={clearCompletedSparks}
                style={[styles.modalBtnPrimary, { backgroundColor: palette.primary }]}
              >
                <Text style={styles.modalBtnPrimaryText}>Clear</Text>
              </Pressable>
            </View>
          </View>
        </View>
      </Modal>

      {/* Full-Screen Focus Mode Modal */}
      <Modal
        visible={focusOpen}
        animationType="fade"
        onRequestClose={() => setFocusOpen(false)}
      >
        <LinearGradient
          colors={[palette.primary, palette.secondary, palette.accent]}
          start={{ x: 0, y: 0 }}
          end={{ x: 1, y: 1 }}
          style={styles.focusContainer}
        >
          <SafeAreaView style={styles.focusSafe}>
            <View style={styles.focusTopBar}>
              <BlurView intensity={40} tint="dark" style={styles.blurPill}>
                <Text style={styles.blurPillText}>FOCUS MODE · {palette.name.toUpperCase()}</Text>
              </BlurView>
              <Pressable onPress={() => setFocusOpen(false)} style={styles.focusCloseBtn}>
                <Text style={styles.focusCloseText}>Close</Text>
              </Pressable>
            </View>

            <View style={styles.focusCenter}>
              <Animated.View
                style={[
                  styles.focusOrbOuter,
                  {
                    opacity: focusOrbOpacity,
                    transform: [{ translateY: focusOrbY }],
                  },
                ]}
              >
                <View style={styles.focusOrbInner} />
              </Animated.View>
              <Text style={styles.focusTitle}>Breathe with Prism</Text>
              <Text style={styles.focusSubtitle}>{palette.mood}</Text>
            </View>
          </SafeAreaView>
        </LinearGradient>
      </Modal>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  root: {
    flex: 1,
  },
  flex: {
    flex: 1,
  },
  scrollContent: {
    padding: 16,
    paddingBottom: 28,
  },
  headerRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginBottom: 14,
  },
  eyebrow: {
    fontSize: 11,
    fontWeight: '700',
    letterSpacing: 1.2,
  },
  screenTitle: {
    fontSize: 28,
    fontWeight: '800',
    marginTop: 2,
  },
  modeBadge: {
    paddingHorizontal: 10,
    paddingVertical: 5,
    borderRadius: 999,
    borderWidth: 1,
  },
  modeBadgeText: {
    fontSize: 12,
    fontWeight: '600',
  },
  heroCard: {
    borderRadius: 24,
    padding: 20,
    overflow: 'hidden',
    marginBottom: 18,
  },
  heroOrbLarge: {
    position: 'absolute',
    right: -18,
    top: -18,
    width: 110,
    height: 110,
    borderRadius: 55,
  },
  heroOrbSmall: {
    position: 'absolute',
    right: 60,
    bottom: -24,
    width: 76,
    height: 76,
    borderRadius: 38,
  },
  heroTopRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginBottom: 18,
  },
  blurPill: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 999,
    overflow: 'hidden',
  },
  blurPillText: {
    color: '#FFFFFF',
    fontSize: 12,
    fontWeight: '700',
  },
  liveDotPill: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: 'rgba(0,0,0,0.22)',
    paddingHorizontal: 10,
    paddingVertical: 5,
    borderRadius: 999,
  },
  liveDot: {
    width: 7,
    height: 7,
    borderRadius: 4,
    marginRight: 6,
  },
  liveDotText: {
    color: '#FFFFFF',
    fontSize: 11,
    fontWeight: '700',
    letterSpacing: 0.8,
  },
  heroScoreLabel: {
    color: 'rgba(255,255,255,0.85)',
    fontSize: 12,
    fontWeight: '700',
    letterSpacing: 1.1,
  },
  heroScoreValue: {
    color: '#FFFFFF',
    fontSize: 52,
    fontWeight: '800',
    lineHeight: 58,
    marginVertical: 4,
  },
  heroSubtitle: {
    color: 'rgba(255,255,255,0.9)',
    fontSize: 13,
    lineHeight: 18,
  },
  sectionLabel: {
    fontSize: 11,
    fontWeight: '700',
    letterSpacing: 1.1,
    marginBottom: 8,
  },
  paletteRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    marginBottom: 16,
    gap: 8,
  },
  palettePill: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 10,
    paddingHorizontal: 8,
    borderRadius: 14,
    borderWidth: 1,
  },
  paletteSwatch: {
    width: 8,
    height: 8,
    borderRadius: 4,
    marginRight: 6,
  },
  palettePillText: {
    fontSize: 13,
    fontWeight: '700',
  },
  card: {
    borderRadius: 18,
    padding: 16,
    borderWidth: 1,
    marginBottom: 14,
  },
  cardHeaderRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  cardTitle: {
    fontSize: 16,
    fontWeight: '700',
  },
  cardHint: {
    fontSize: 13,
  },
  percentText: {
    fontSize: 16,
    fontWeight: '800',
  },
  smallGhostBtn: {
    paddingHorizontal: 10,
    paddingVertical: 5,
    borderRadius: 10,
    borderWidth: 1,
  },
  smallGhostBtnText: {
    fontSize: 12,
    fontWeight: '600',
  },
  progressTrack: {
    height: 10,
    borderRadius: 999,
    overflow: 'hidden',
    marginTop: 12,
  },
  progressFill: {
    height: 10,
    borderRadius: 999,
  },
  surgePressable: {
    marginTop: 14,
  },
  surgeButton: {
    borderRadius: 14,
    paddingVertical: 13,
    alignItems: 'center',
  },
  surgeButtonText: {
    color: '#FFFFFF',
    fontSize: 15,
    fontWeight: '800',
  },
  switchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingVertical: 4,
  },
  switchTitle: {
    fontSize: 15,
    fontWeight: '600',
  },
  switchSub: {
    fontSize: 12,
    marginTop: 2,
  },
  divider: {
    height: 1,
    marginVertical: 12,
  },
  sparksHeaderWrap: {
    marginBottom: 4,
  },
  primaryChipBtn: {
    paddingHorizontal: 14,
    paddingVertical: 8,
    borderRadius: 999,
  },
  primaryChipBtnText: {
    color: '#FFFFFF',
    fontSize: 13,
    fontWeight: '700',
  },
  sparksActionRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginTop: 12,
  },
  clearBtn: {
    paddingHorizontal: 10,
    paddingVertical: 5,
    borderRadius: 10,
    borderWidth: 1,
  },
  clearBtnText: {
    fontSize: 12,
    fontWeight: '700',
  },
  sparkCard: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 14,
    borderRadius: 16,
    borderWidth: 1,
    marginBottom: 10,
  },
  sparkCheck: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: 12,
  },
  sparkCheckMark: {
    color: '#FFFFFF',
    fontSize: 13,
    fontWeight: '800',
  },
  sparkTitle: {
    fontSize: 15,
    fontWeight: '600',
  },
  sparkMetaRow: {
    flexDirection: 'row',
    alignItems: 'center',
    marginTop: 6,
    gap: 8,
  },
  categoryBadge: {
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 999,
  },
  categoryBadgeText: {
    color: '#FFFFFF',
    fontSize: 11,
    fontWeight: '700',
  },
  sparkStatusText: {
    fontSize: 12,
  },
  labPreviewCard: {
    padding: 18,
    marginBottom: 16,
    overflow: 'hidden',
    minHeight: 156,
    justifyContent: 'center',
  },
  labFloatingOrb: {
    position: 'absolute',
    top: 16,
    alignSelf: 'center',
    width: 64,
    height: 64,
    borderRadius: 32,
  },
  labBlurBox: {
    padding: 16,
    overflow: 'hidden',
  },
  labSolidBox: {
    padding: 16,
    backgroundColor: 'rgba(0,0,0,0.22)',
  },
  labPreviewTitle: {
    color: '#FFFFFF',
    fontSize: 18,
    fontWeight: '800',
  },
  labPreviewMeta: {
    color: 'rgba(255,255,255,0.9)',
    fontSize: 13,
    marginTop: 4,
  },
  labSpinnerRow: {
    flexDirection: 'row',
    alignItems: 'center',
    marginTop: 10,
    gap: 8,
  },
  labSpinnerText: {
    color: '#FFFFFF',
    fontSize: 12,
    fontWeight: '600',
  },
  controlLabel: {
    fontSize: 11,
    fontWeight: '700',
    letterSpacing: 1.1,
    marginBottom: 8,
  },
  presetRow: {
    flexDirection: 'row',
    gap: 8,
  },
  presetPill: {
    flex: 1,
    paddingVertical: 9,
    borderRadius: 12,
    borderWidth: 1,
    alignItems: 'center',
  },
  presetPillText: {
    fontSize: 13,
    fontWeight: '700',
  },
  platformBadge: {
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 999,
  },
  platformBadgeText: {
    color: '#FFFFFF',
    fontSize: 12,
    fontWeight: '700',
  },
  tabBar: {
    flexDirection: 'row',
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderTopWidth: 1,
    gap: 8,
  },
  tabItem: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 10,
    borderRadius: 14,
    gap: 6,
  },
  tabGlyph: {
    fontSize: 15,
    fontWeight: '700',
  },
  tabLabel: {
    fontSize: 13,
    fontWeight: '700',
  },
  modalBackdrop: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.55)',
    justifyContent: 'center',
    padding: 20,
  },
  modalSheet: {
    borderRadius: 22,
    padding: 20,
    borderWidth: 1,
    overflow: 'hidden',
  },
  modalAccentBar: {
    height: 6,
    borderRadius: 999,
    marginBottom: 14,
  },
  modalTitle: {
    fontSize: 20,
    fontWeight: '800',
  },
  textInput: {
    borderRadius: 12,
    borderWidth: 1,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 15,
  },
  modalActions: {
    flexDirection: 'row',
    justifyContent: 'flex-end',
    marginTop: 18,
    gap: 10,
  },
  modalBtnSecondary: {
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderRadius: 12,
    borderWidth: 1,
  },
  modalBtnSecondaryText: {
    fontSize: 14,
    fontWeight: '700',
  },
  modalBtnPrimary: {
    paddingHorizontal: 18,
    paddingVertical: 10,
    borderRadius: 12,
  },
  modalBtnPrimaryText: {
    color: '#FFFFFF',
    fontSize: 14,
    fontWeight: '700',
  },
  focusContainer: {
    flex: 1,
  },
  focusSafe: {
    flex: 1,
    padding: 20,
  },
  focusTopBar: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  focusCloseBtn: {
    backgroundColor: 'rgba(0,0,0,0.28)',
    paddingHorizontal: 14,
    paddingVertical: 8,
    borderRadius: 999,
  },
  focusCloseText: {
    color: '#FFFFFF',
    fontSize: 13,
    fontWeight: '700',
  },
  focusCenter: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  focusOrbOuter: {
    width: 170,
    height: 170,
    borderRadius: 85,
    backgroundColor: 'rgba(255,255,255,0.28)',
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 28,
  },
  focusOrbInner: {
    width: 104,
    height: 104,
    borderRadius: 52,
    backgroundColor: 'rgba(255,255,255,0.6)',
  },
  focusTitle: {
    color: '#FFFFFF',
    fontSize: 28,
    fontWeight: '800',
  },
  focusSubtitle: {
    color: 'rgba(255,255,255,0.9)',
    fontSize: 15,
    marginTop: 6,
    fontWeight: '600',
  },
});
