package io.pierrotws.passel

import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle

class MainActivity : ComponentActivity() {
    private val model: VaultViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Secrets stay out of screenshots and the recent-apps preview.
        // Debug builds allow captures, for testing.
        if (!BuildConfig.DEBUG) {
            window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        }
        enableEdgeToEdge()
        val prompts = (application as PasselApp).prompts
        setContent {
            PasselTheme {
                PasselScreen(model)
                val request by prompts.request.collectAsStateWithLifecycle()
                request?.let { PassphraseDialog(it, onAnswer = { answer -> prompts.answer(it, answer) }) }
            }
        }
    }
}

@Composable
fun PasselTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val context = LocalContext.current
    val colors = when {
        Build.VERSION.SDK_INT >= 31 && dark -> dynamicDarkColorScheme(context)
        Build.VERSION.SDK_INT >= 31 -> dynamicLightColorScheme(context)
        dark -> darkColorScheme()
        else -> lightColorScheme()
    }
    MaterialTheme(colorScheme = colors, content = content)
}

@Composable
private fun PasselScreen(model: VaultViewModel) {
    val state by model.state.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(model) { model.messages.collect { snackbar.showSnackbar(it) } }

    val open = state.open
    BackHandler(enabled = open != null || state.creating) { model.close() }

    Box(Modifier.fillMaxSize()) {
        when {
            state.starting -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
            state.addingVault || !state.hasVault -> SetupWizard(state, model)
            open != null -> EntryScreen(open, state.busy, model)
            state.creating -> NewEntryScreen(state.busy, model)
            else -> VaultScreen(state, model)
        }
        SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter))
    }
}

@Composable
private fun PassphraseDialog(request: PromptRequest, onAnswer: (String?) -> Unit) {
    var passphrase by remember(request) { mutableStateOf("") }
    var confirm by remember(request) { mutableStateOf("") }
    val creating = request is PromptRequest.NewKey
    val mismatch = creating && passphrase != confirm
    AlertDialog(
        onDismissRequest = { onAnswer(null) },
        title = { Text(if (creating) "Choose a passphrase" else "Unlock your key") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                when (request) {
                    is PromptRequest.Unlock -> {
                        Text(request.key.userId ?: request.key.fingerprint)
                        if (request.retry) Text("Wrong passphrase, try again", color = MaterialTheme.colorScheme.error)
                    }
                    is PromptRequest.NewKey -> Text(
                        "It protects the key for ${request.userId} on this phone and in its backups.",
                    )
                }
                PasswordField(passphrase, { passphrase = it }, "Passphrase")
                if (creating) PasswordField(confirm, { confirm = it }, "Repeat it")
            }
        },
        confirmButton = {
            TextButton(onClick = { onAnswer(passphrase) }, enabled = passphrase.isNotEmpty() && !mismatch) {
                Text(if (creating) "Create" else "Unlock")
            }
        },
        dismissButton = { TextButton(onClick = { onAnswer(null) }) { Text("Cancel") } },
    )
}

/**
 * A masked field for passphrases and tokens.
 *
 * It keeps the deprecated `PasswordVisualTransformation` on purpose: its
 * replacement, `OutlinedSecureTextField` (Compose 1.13), hands the typed
 * text in clear to accessibility services, where this one only exposes
 * the dots (checked with a UI Automator dump on Android 16). Revisit
 * when the secure field masks its accessibility text.
 */
@Suppress("DEPRECATION")
@Composable
fun PasswordField(
    value: String,
    onChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        enabled = enabled,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        modifier = modifier,
    )
}
