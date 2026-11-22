package com.example.prism

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ElevatedButton
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

class PrismActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { PrismApp() }
    }
}

private data class PrismPalette(
    val name: String,
    val mood: String,
    val primary: Color,
    val secondary: Color,
    val accent: Color,
    val lightContainer: Color,
    val onLightContainer: Color,
    val darkContainer: Color,
    val onDarkContainer: Color,
)

private val PrismPalettes = listOf(
    PrismPalette(
        name = "Coral",
        mood = "Sunset Energy",
        primary = Color(0xFFFF5E62),
        secondary = Color(0xFFFF9966),
        accent = Color(0xFFFFD166),
        lightContainer = Color(0xFFFFE0E0),
        onLightContainer = Color(0xFF5C1013),
        darkContainer = Color(0xFF6B1E21),
        onDarkContainer = Color(0xFFFFDADA),
    ),
    PrismPalette(
        name = "Mint",
        mood = "Fresh Focus",
        primary = Color(0xFF00B894),
        secondary = Color(0xFF00CEC9),
        accent = Color(0xFF55EFC4),
        lightContainer = Color(0xFFD2F7EE),
        onLightContainer = Color(0xFF003D31),
        darkContainer = Color(0xFF004F3F),
        onDarkContainer = Color(0xFFA7F3E2),
    ),
    PrismPalette(
        name = "Berry",
        mood = "Electric Flow",
        primary = Color(0xFF8E44AD),
        secondary = Color(0xFFE84393),
        accent = Color(0xFFFD79A8),
        lightContainer = Color(0xFFF3DCF9),
        onLightContainer = Color(0xFF3E1052),
        darkContainer = Color(0xFF4F1D66),
        onDarkContainer = Color(0xFFF3D5FF),
    ),
    PrismPalette(
        name = "Solar",
        mood = "Golden Spark",
        primary = Color(0xFFF39C12),
        secondary = Color(0xFFE67E22),
        accent = Color(0xFFF1C40F),
        lightContainer = Color(0xFFFFECC7),
        onLightContainer = Color(0xFF4D2D00),
        darkContainer = Color(0xFF633B00),
        onDarkContainer = Color(0xFFFFE0A3),
    ),
)

private data class SparkItem(
    val id: Int,
    val title: String,
    val category: String,
    val done: Boolean = false,
)

private val DefaultSparks = listOf(
    SparkItem(1, "Sketch a cloud", "Play", done = true),
    SparkItem(2, "Capture a sunset", "Make", done = false),
    SparkItem(3, "Ship a tiny delight", "Make", done = false),
    SparkItem(4, "Hydrate & stretch", "Move", done = false),
)

private val RandomSparkPool = listOf(
    "Doodle a mascot" to "Play",
    "Take a 5-min walk" to "Move",
    "Write a haiku" to "Calm",
    "Mix a new palette" to "Make",
    "Breathe deeply x3" to "Calm",
)

private enum class PrismTab(val label: String, val glyph: String) {
    Pulse("Pulse", "◉"),
    Sparks("Sparks", "✦"),
    Lab("Lab", "◈"),
}

private enum class LabCardStyle(val label: String) {
    Filled("Filled"),
    Elevated("Elevated"),
    Outlined("Outlined"),
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PrismApp() {
    val dark = isSystemInDarkTheme()
    var paletteIndex by remember { mutableIntStateOf(0) }
    var activeTab by remember { mutableStateOf(PrismTab.Pulse) }

    // Pulse state
    var vibeScore by remember { mutableIntStateOf(72) }
    var energy by remember { mutableStateOf(0.68f) }
    var glowMode by remember { mutableStateOf(true) }

    // Sparks state
    var sparks by remember { mutableStateOf(DefaultSparks) }
    var nextSparkId by remember { mutableIntStateOf(5) }
    var randomCursor by remember { mutableIntStateOf(0) }
    var draftSpark by remember { mutableStateOf("") }
    var sparksPage by remember { mutableIntStateOf(0) }
    val sparksPageSize = 4

    // Lab state
    var labCornerDp by remember { mutableStateOf(20f) }
    var labAlpha by remember { mutableStateOf(1f) }
    var labStyle by remember { mutableStateOf(LabCardStyle.Elevated) }
    var labTint by remember { mutableStateOf(true) }
    var labHeadline by remember { mutableStateOf("Prism Studio") }
    var labFeedback by remember { mutableStateOf("Tap any M3 button below") }
    var labTapCount by remember { mutableIntStateOf(0) }

    val palette = PrismPalettes[paletteIndex]
    val colorScheme = if (dark) {
        darkColorScheme(
            primary = palette.primary,
            onPrimary = Color.White,
            primaryContainer = palette.darkContainer,
            onPrimaryContainer = palette.onDarkContainer,
            secondary = palette.secondary,
            onSecondary = Color.White,
            secondaryContainer = palette.darkContainer,
            onSecondaryContainer = palette.onDarkContainer,
            tertiary = palette.accent,
            onTertiary = Color(0xFF1A1A1A),
        )
    } else {
        lightColorScheme(
            primary = palette.primary,
            onPrimary = Color.White,
            primaryContainer = palette.lightContainer,
            onPrimaryContainer = palette.onLightContainer,
            secondary = palette.secondary,
            onSecondary = Color.White,
            secondaryContainer = palette.lightContainer,
            onSecondaryContainer = palette.onLightContainer,
            tertiary = palette.accent,
            onTertiary = Color(0xFF1A1A1A),
        )
    }

    MaterialTheme(colorScheme = colorScheme) {
        Scaffold(
            topBar = {
                CenterAlignedTopAppBar(
                    title = {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Box(
                                Modifier
                                    .size(12.dp)
                                    .clip(CircleShape)
                                    .background(palette.primary)
                            )
                            Text("Prism", fontWeight = FontWeight.Bold)
                        }
                    },
                    actions = {
                        Surface(
                            modifier = Modifier.padding(end = 12.dp),
                            shape = CircleShape,
                            color = MaterialTheme.colorScheme.primaryContainer,
                            contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                        ) {
                            Text(
                                text = palette.name,
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                                style = MaterialTheme.typography.labelMedium,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    },
                )
            },
            bottomBar = {
                PrismBottomBar(
                    activeTab = activeTab,
                    onSelect = { activeTab = it },
                )
            },
            floatingActionButton = {
                when (activeTab) {
                    PrismTab.Pulse -> {
                        ExtendedFloatingActionButton(
                            onClick = {
                                vibeScore += 5
                                energy = (energy + 0.08f).coerceAtMost(1f)
                            },
                            modifier = Modifier.padding(bottom = 80.dp),
                            containerColor = palette.primary,
                            contentColor = Color.White,
                        ) {
                            Text("✦ Boost +5", fontWeight = FontWeight.SemiBold)
                        }
                    }
                    PrismTab.Sparks -> {
                        ExtendedFloatingActionButton(
                            onClick = {
                                val (title, cat) = RandomSparkPool[randomCursor % RandomSparkPool.size]
                                randomCursor++
                                val updated = sparks + SparkItem(nextSparkId++, title, cat, done = false)
                                sparks = updated
                                sparksPage = (updated.size - 1) / sparksPageSize
                            },
                            modifier = Modifier.padding(bottom = 80.dp),
                            containerColor = palette.primary,
                            contentColor = Color.White,
                        ) {
                            Text("+ Spark", fontWeight = FontWeight.SemiBold)
                        }
                    }
                    PrismTab.Lab -> {}
                }
            },
        ) { innerPadding ->
            Box(
                Modifier
                    .fillMaxSize()
                    .padding(innerPadding)
                    .padding(horizontal = 16.dp, vertical = 8.dp)
            ) {
                when (activeTab) {
                    PrismTab.Pulse -> PulseTab(
                        palette = palette,
                        paletteIndex = paletteIndex,
                        onSelectPalette = { paletteIndex = it },
                        vibeScore = vibeScore,
                        energy = energy,
                        glowMode = glowMode,
                        onBoost = {
                            vibeScore += 10
                            energy = (energy + 0.12f).coerceAtMost(1f)
                        },
                        onReset = {
                            vibeScore = 50
                            energy = 0.50f
                        },
                        onEnergyChange = { energy = it },
                        onGlowChange = { glowMode = it },
                    )
                    PrismTab.Sparks -> SparksTab(
                        palette = palette,
                        sparks = sparks,
                        page = sparksPage,
                        pageSize = sparksPageSize,
                        onPageChange = { sparksPage = it },
                        draft = draftSpark,
                        onDraftChange = { draftSpark = it },
                        onAddDraft = {
                            val clean = draftSpark.trim()
                            if (clean.isNotEmpty()) {
                                val updated = sparks + SparkItem(nextSparkId++, clean, "Play", done = false)
                                sparks = updated
                                sparksPage = (updated.size - 1) / sparksPageSize
                                draftSpark = ""
                            }
                        },
                        onToggle = { id ->
                            sparks = sparks.map { if (it.id == id) it.copy(done = !it.done) else it }
                        },
                        onClearDone = {
                            val remaining = sparks.filterNot { it.done }
                            sparks = remaining
                            val maxPage = ((remaining.size - 1).coerceAtLeast(0)) / sparksPageSize
                            if (sparksPage > maxPage) sparksPage = maxPage
                        },
                        onRestore = {
                            sparks = DefaultSparks
                            sparksPage = 0
                        },
                    )
                    PrismTab.Lab -> LabTab(
                        palette = palette,
                        cornerDp = labCornerDp,
                        onCornerChange = { labCornerDp = it },
                        cardAlpha = labAlpha,
                        onAlphaChange = { labAlpha = it },
                        style = labStyle,
                        onStyleChange = { labStyle = it },
                        tinted = labTint,
                        onTintChange = { labTint = it },
                        headline = labHeadline,
                        onHeadlineChange = { labHeadline = it },
                        feedback = labFeedback,
                        tapCount = labTapCount,
                        onButtonTap = { name ->
                            labTapCount++
                            labFeedback = "$name tapped (#$labTapCount)"
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun PrismBottomBar(
    activeTab: PrismTab,
    onSelect: (PrismTab) -> Unit,
) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.surfaceContainer,
        shadowElevation = 4.dp,
    ) {
        Column(Modifier.fillMaxWidth().navigationBarsPadding()) {
            HorizontalDivider()
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PrismTab.entries.forEach { tab ->
                    val selected = tab == activeTab
                    Surface(
                        onClick = { onSelect(tab) },
                        modifier = Modifier.weight(1f),
                        shape = CircleShape,
                        color = if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent,
                        contentColor = if (selected) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant,
                    ) {
                        Row(
                            modifier = Modifier.padding(vertical = 8.dp, horizontal = 8.dp),
                            horizontalArrangement = Arrangement.Center,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = "${tab.glyph} ${tab.label}",
                                style = MaterialTheme.typography.labelLarge,
                                fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun PulseTab(
    palette: PrismPalette,
    paletteIndex: Int,
    onSelectPalette: (Int) -> Unit,
    vibeScore: Int,
    energy: Float,
    glowMode: Boolean,
    onBoost: () -> Unit,
    onReset: () -> Unit,
    onEnergyChange: (Float) -> Unit,
    onGlowChange: (Boolean) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Colorful Hero Vibe Card with layered orbs
        Card(
            onClick = onBoost,
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(24.dp),
            colors = CardDefaults.cardColors(
                containerColor = palette.primary,
                contentColor = Color.White,
            ),
            elevation = CardDefaults.cardElevation(defaultElevation = if (glowMode) 6.dp else 2.dp),
        ) {
            Box(Modifier.fillMaxWidth().height(168.dp)) {
                // Layered decorative orbs
                Box(
                    Modifier
                        .align(Alignment.TopEnd)
                        .offset(x = 24.dp, y = (-24).dp)
                        .size(120.dp)
                        .shadow(if (glowMode) 8.dp else 0.dp, CircleShape, clip = false)
                        .clip(CircleShape)
                        .background(palette.accent.copy(alpha = if (glowMode) 0.45f else 0.22f))
                )
                Box(
                    Modifier
                        .align(Alignment.BottomEnd)
                        .offset(x = (-20).dp, y = 24.dp)
                        .size(88.dp)
                        .clip(CircleShape)
                        .background(palette.secondary.copy(alpha = if (glowMode) 0.55f else 0.28f))
                )
                Box(
                    Modifier
                        .align(Alignment.BottomStart)
                        .offset(x = (-18).dp, y = 26.dp)
                        .size(72.dp)
                        .clip(CircleShape)
                        .background(Color.White.copy(alpha = 0.14f))
                )

                Column(
                    modifier = Modifier.fillMaxSize().padding(18.dp),
                    verticalArrangement = Arrangement.SpaceBetween,
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Surface(
                            shape = CircleShape,
                            color = Color.White.copy(alpha = 0.22f),
                            contentColor = Color.White,
                        ) {
                            Text(
                                text = palette.mood,
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                                style = MaterialTheme.typography.labelMedium,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                        Text(
                            text = if (glowMode) "GLOW ON" else "ZEN MODE",
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Bold,
                            color = Color.White.copy(alpha = 0.9f),
                        )
                    }

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.Bottom,
                    ) {
                        Column {
                            Text(
                                text = "${palette.name} Studio",
                                style = MaterialTheme.typography.headlineSmall,
                                fontWeight = FontWeight.Bold,
                                color = Color.White,
                            )
                            Text(
                                text = "Tap card to surge energy",
                                style = MaterialTheme.typography.bodySmall,
                                color = Color.White.copy(alpha = 0.85f),
                            )
                        }
                        Column(horizontalAlignment = Alignment.End) {
                            Text(
                                text = "$vibeScore",
                                style = MaterialTheme.typography.displaySmall,
                                fontWeight = FontWeight.Bold,
                                color = Color.White,
                            )
                            Text(
                                text = "VIBE SCORE",
                                style = MaterialTheme.typography.labelSmall,
                                color = Color.White.copy(alpha = 0.85f),
                            )
                        }
                    }
                }
            }
        }

        // 4 Palette Selector Pills
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            PrismPalettes.forEachIndexed { idx, item ->
                val selected = idx == paletteIndex
                Surface(
                    onClick = { onSelectPalette(idx) },
                    modifier = Modifier.weight(1f),
                    shape = CircleShape,
                    color = if (selected) item.primary else MaterialTheme.colorScheme.surfaceContainerHigh,
                    contentColor = if (selected) Color.White else MaterialTheme.colorScheme.onSurface,
                    border = if (selected) null else BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
                ) {
                    Row(
                        modifier = Modifier.padding(vertical = 8.dp, horizontal = 6.dp),
                        horizontalArrangement = Arrangement.Center,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            Modifier
                                .size(8.dp)
                                .clip(CircleShape)
                                .background(if (selected) Color.White else item.primary)
                        )
                        Spacer(Modifier.width(5.dp))
                        Text(
                            text = item.name,
                            style = MaterialTheme.typography.labelMedium,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
        }

        // Energy Mixer ElevatedCard
        ElevatedCard(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(20.dp),
        ) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = "Energy Mixer",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                    )
                    Surface(
                        shape = CircleShape,
                        color = MaterialTheme.colorScheme.primaryContainer,
                        contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                    ) {
                        Text(
                            text = "${(energy * 100).roundToInt()}%",
                            modifier = Modifier.padding(horizontal = 10.dp, vertical = 3.dp),
                            style = MaterialTheme.typography.labelMedium,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }

                // Custom progress bar
                Box(
                    Modifier
                        .fillMaxWidth()
                        .height(12.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.surfaceContainerHighest)
                ) {
                    Box(
                        Modifier
                            .fillMaxWidth(energy.coerceIn(0.04f, 1f))
                            .height(12.dp)
                            .clip(CircleShape)
                            .background(if (glowMode) palette.primary else palette.secondary)
                    )
                }

                Slider(
                    value = energy,
                    onValueChange = onEnergyChange,
                    modifier = Modifier.fillMaxWidth(),
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column {
                        Text(
                            text = if (glowMode) "Glow Mode" else "Zen Mode",
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Text(
                            text = if (glowMode) "Vibrant orbs & elevated shadow" else "Calm muted surface",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(checked = glowMode, onCheckedChange = onGlowChange)
                }

                HorizontalDivider()

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Button(
                        onClick = onBoost,
                        modifier = Modifier.weight(1f),
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text("Boost +10")
                    }
                    FilledTonalButton(
                        onClick = { onEnergyChange(1f) },
                        modifier = Modifier.weight(1f),
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text("Max")
                    }
                    OutlinedButton(
                        onClick = onReset,
                        modifier = Modifier.weight(1f),
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Text("Reset")
                    }
                }
            }
        }
    }
}

@Composable
private fun SparksTab(
    palette: PrismPalette,
    sparks: List<SparkItem>,
    page: Int,
    pageSize: Int,
    onPageChange: (Int) -> Unit,
    draft: String,
    onDraftChange: (String) -> Unit,
    onAddDraft: () -> Unit,
    onToggle: (Int) -> Unit,
    onClearDone: () -> Unit,
    onRestore: () -> Unit,
) {
    val doneCount = sparks.count { it.done }
    val progress = if (sparks.isEmpty()) 0f else doneCount.toFloat() / sparks.size.toFloat()
    val pageCount = if (sparks.isEmpty()) 1 else (sparks.size + pageSize - 1) / pageSize
    val safePage = page.coerceIn(0, pageCount - 1)
    val pageSparks = sparks.drop(safePage * pageSize).take(pageSize)

    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        // Progress Header Card
        Card(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(18.dp),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.primaryContainer,
                contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
            ),
        ) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(14.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = "Daily Sparks",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = "$doneCount / ${sparks.size} done",
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
                Box(
                    Modifier
                        .fillMaxWidth()
                        .height(10.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.surface.copy(alpha = 0.5f))
                ) {
                    if (progress > 0f) {
                        Box(
                            Modifier
                                .fillMaxWidth(progress.coerceIn(0.05f, 1f))
                                .height(10.dp)
                                .clip(CircleShape)
                                .background(palette.primary)
                        )
                    }
                }
            }
        }

        // Inline TextField + Add Button
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextField(
                value = draft,
                onValueChange = onDraftChange,
                modifier = Modifier.weight(1f),
                singleLine = true,
            )
            Button(
                onClick = onAddDraft,
                enabled = draft.trim().isNotEmpty(),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 10.dp),
            ) {
                Text("Add")
            }
        }

        // Spark Items
        if (sparks.isEmpty()) {
            OutlinedCard(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(16.dp),
            ) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(20.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text("All sparks cleared!", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
                    Text(
                        "Tap + Spark or Restore to refill.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        } else {
            pageSparks.forEach { spark ->
                val cardColor = if (spark.done) {
                    MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.65f)
                } else {
                    MaterialTheme.colorScheme.surfaceContainerLow
                }
                Card(
                    onClick = { onToggle(spark.id) },
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(14.dp),
                    colors = CardDefaults.cardColors(containerColor = cardColor),
                    border = if (spark.done) BorderStroke(1.dp, palette.primary) else BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Box(
                            modifier = Modifier
                                .size(24.dp)
                                .clip(CircleShape)
                                .background(if (spark.done) palette.primary else Color.Transparent)
                                .border(2.dp, if (spark.done) palette.primary else MaterialTheme.colorScheme.outline, CircleShape),
                            contentAlignment = Alignment.Center,
                        ) {
                            if (spark.done) {
                                Text("✓", style = MaterialTheme.typography.labelSmall, color = Color.White, fontWeight = FontWeight.Bold)
                            }
                        }
                        Text(
                            text = spark.title,
                            modifier = Modifier.weight(1f),
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = if (spark.done) FontWeight.Normal else FontWeight.Medium,
                        )
                        Surface(
                            shape = CircleShape,
                            color = if (spark.done) palette.primary else MaterialTheme.colorScheme.secondaryContainer,
                            contentColor = if (spark.done) Color.White else MaterialTheme.colorScheme.onSecondaryContainer,
                        ) {
                            Text(
                                text = spark.category,
                                modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }
            }
        }

        // Pagination (when Sparks span multiple pages)
        if (pageCount > 1) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedButton(
                    onClick = { onPageChange((safePage - 1).coerceAtLeast(0)) },
                    enabled = safePage > 0,
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
                ) {
                    Text("Prev")
                }
                Text(
                    text = "Page ${safePage + 1} / $pageCount",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                OutlinedButton(
                    onClick = { onPageChange((safePage + 1).coerceAtMost(pageCount - 1)) },
                    enabled = safePage < pageCount - 1,
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
                ) {
                    Text("Next")
                }
            }
        }

        // Actions Row
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onClearDone, enabled = doneCount > 0) {
                Text("Clear done")
            }
            if (sparks.size < DefaultSparks.size) {
                TextButton(onClick = onRestore) {
                    Text("Restore defaults")
                }
            }
        }
    }
}

@Composable
private fun LabTab(
    palette: PrismPalette,
    cornerDp: Float,
    onCornerChange: (Float) -> Unit,
    cardAlpha: Float,
    onAlphaChange: (Float) -> Unit,
    style: LabCardStyle,
    onStyleChange: (LabCardStyle) -> Unit,
    tinted: Boolean,
    onTintChange: (Boolean) -> Unit,
    headline: String,
    onHeadlineChange: (String) -> Unit,
    feedback: String,
    tapCount: Int,
    onButtonTap: (String) -> Unit,
) {
    val shape = RoundedCornerShape(cornerDp.dp)
    val previewModifier = Modifier.fillMaxWidth().alpha(cardAlpha)
    val container = if (tinted) palette.primary else MaterialTheme.colorScheme.surfaceContainerHigh
    val content = if (tinted) Color.White else MaterialTheme.colorScheme.onSurface

    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        // Live Interactive Preview Card
        val previewContent: @Composable () -> Unit = {
            Column(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = headline.ifBlank { "Prism Studio" },
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold,
                        color = content,
                    )
                    Surface(
                        shape = CircleShape,
                        color = if (tinted) Color.White.copy(alpha = 0.22f) else MaterialTheme.colorScheme.primaryContainer,
                        contentColor = if (tinted) Color.White else MaterialTheme.colorScheme.onPrimaryContainer,
                    ) {
                        Text(
                            text = style.label,
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
                Text(
                    text = "Radius ${cornerDp.roundToInt()}dp · Alpha ${(cardAlpha * 100).roundToInt()}% · ${palette.name}",
                    style = MaterialTheme.typography.bodySmall,
                    color = content.copy(alpha = 0.85f),
                )
            }
        }

        when (style) {
            LabCardStyle.Filled -> Card(
                modifier = previewModifier,
                shape = shape,
                colors = CardDefaults.cardColors(containerColor = container, contentColor = content),
            ) { previewContent() }
            LabCardStyle.Elevated -> ElevatedCard(
                modifier = previewModifier,
                shape = shape,
                colors = CardDefaults.elevatedCardColors(containerColor = container, contentColor = content),
                elevation = CardDefaults.elevatedCardElevation(defaultElevation = 6.dp),
            ) { previewContent() }
            LabCardStyle.Outlined -> OutlinedCard(
                modifier = previewModifier,
                shape = shape,
                colors = CardDefaults.outlinedCardColors(
                    containerColor = if (tinted) palette.primary.copy(alpha = 0.14f) else MaterialTheme.colorScheme.surface,
                    contentColor = MaterialTheme.colorScheme.onSurface,
                ),
                border = BorderStroke(2.dp, palette.primary),
            ) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = headline.ifBlank { "Prism Studio" },
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.Bold,
                        )
                        Surface(
                            shape = CircleShape,
                            color = MaterialTheme.colorScheme.primaryContainer,
                            contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                        ) {
                            Text(
                                text = style.label,
                                modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.Bold,
                            )
                        }
                    }
                    Text(
                        text = "Radius ${cornerDp.roundToInt()}dp · Alpha ${(cardAlpha * 100).roundToInt()}% · ${palette.name}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        // Controls Card
        ElevatedCard(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(18.dp),
        ) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                // Style Pills + Tint Switch
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    LabCardStyle.entries.forEach { option ->
                        val selected = option == style
                        Surface(
                            onClick = { onStyleChange(option) },
                            modifier = Modifier.weight(1f),
                            shape = CircleShape,
                            color = if (selected) palette.primary else MaterialTheme.colorScheme.surfaceContainerHighest,
                            contentColor = if (selected) Color.White else MaterialTheme.colorScheme.onSurface,
                        ) {
                            Box(Modifier.padding(vertical = 6.dp), contentAlignment = Alignment.Center) {
                                Text(option.label, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }
                    Spacer(Modifier.width(4.dp))
                    Text("Tint", style = MaterialTheme.typography.labelMedium)
                    Switch(checked = tinted, onCheckedChange = onTintChange)
                }

                // Corner Radius Slider
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("Corner radius", style = MaterialTheme.typography.labelMedium)
                    Text("${cornerDp.roundToInt()} dp", style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                }
                Slider(
                    value = cornerDp,
                    onValueChange = onCornerChange,
                    valueRange = 4f..32f,
                    modifier = Modifier.fillMaxWidth(),
                )

                // Opacity Slider
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("Opacity", style = MaterialTheme.typography.labelMedium)
                    Text("${(cardAlpha * 100).roundToInt()}%", style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                }
                Slider(
                    value = cardAlpha,
                    onValueChange = onAlphaChange,
                    valueRange = 0.35f..1f,
                    modifier = Modifier.fillMaxWidth(),
                )

                // Headline TextField
                TextField(
                    value = headline,
                    onValueChange = onHeadlineChange,
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                )
            }
        }

        // Material 3 Button Gallery
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("M3 Button Gallery", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
                Surface(
                    shape = CircleShape,
                    color = MaterialTheme.colorScheme.primaryContainer,
                    contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                ) {
                    Text(
                        text = feedback,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 3.dp),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Medium,
                    )
                }
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Button(
                    onClick = { onButtonTap("Filled") },
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 8.dp),
                ) { Text("Filled") }
                ElevatedButton(
                    onClick = { onButtonTap("Elevated") },
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 8.dp),
                ) { Text("Elevated") }
                FilledTonalButton(
                    onClick = { onButtonTap("Tonal") },
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 8.dp),
                ) { Text("Tonal") }
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedButton(
                    onClick = { onButtonTap("Outlined") },
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 8.dp),
                ) { Text("Outlined") }
                TextButton(
                    onClick = { onButtonTap("Text") },
                    modifier = Modifier.weight(1f),
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 8.dp),
                ) { Text("Text") }
                Button(
                    onClick = {},
                    enabled = false,
                    modifier = Modifier.weight(1f),
                    colors = ButtonDefaults.buttonColors(),
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 8.dp),
                ) { Text("Disabled") }
            }
        }
    }
}
