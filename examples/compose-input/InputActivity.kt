package com.example.input

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

class InputActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                var name by remember { mutableStateOf("Name") }
                var note by remember { mutableStateOf("Note") }
                var volume by remember { mutableStateOf(25f) }
                Column(Modifier.padding(start = 24.dp, top = 104.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Compose input")
                    BasicTextField(
                        value = name,
                        onValueChange = { name = it },
                        modifier = Modifier.width(240.dp).background(Color(0xFFF1F0F5)).padding(8.dp),
                        singleLine = true,
                    )
                    TextField(note, { note = it }, Modifier.width(240.dp), singleLine = true)
                    Text("Volume ${volume.toInt()}")
                    Slider(volume, { volume = it }, Modifier.width(240.dp), valueRange = 0f..100f, steps = 3)
                }
            }
        }
    }
}
