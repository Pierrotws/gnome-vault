package io.pierrotws.passel

import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricPrompt
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Checkbox
import androidx.compose.ui.semantics.Role
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import javax.crypto.Cipher
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle

// A FragmentActivity: androidx's BiometricPrompt needs one.
class MainActivity : FragmentActivity() {
    private val model: VaultViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Recreated when the language changes: pick it up before drawing.
        I18n.apply(resources.configuration)
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
                DriveConsent(model)
                val request by prompts.request.collectAsStateWithLifecycle()
                when (val asked = request) {
                    null -> {}
                    is PromptRequest.Biometric -> LaunchedEffect(asked) {
                        authenticate(
                            title = tr("Unlock your key"),
                            subtitle = asked.key.userId ?: asked.key.fingerprint,
                            // Translators: button of the biometric prompt, to type the passphrase instead.
                            negative = tr("Use the passphrase"),
                            cipher = asked.cipher,
                        ) { prompts.answerBiometric(asked, it) }
                    }
                    else -> PassphraseDialog(asked, onAnswer = { answer -> prompts.answer(asked, answer) })
                }
                val enrollment by prompts.enrollment.collectAsStateWithLifecycle()
                enrollment?.let { enroll ->
                    LaunchedEffect(enroll) {
                        authenticate(
                            title = tr("Turn on biometric unlock"),
                            subtitle = tr("Confirm to unlock this key with biometrics next time"),
                            negative = tr("Cancel"),
                            cipher = enroll.cipher,
                        ) { model.finishEnrollment(enroll, it) }
                    }
                }
            }
        }
    }

    /** Shows Google's account and consent screen when Drive access needs it. */
    @Composable
    private fun DriveConsent(model: VaultViewModel) {
        val state by model.state.collectAsStateWithLifecycle()
        val launcher = rememberLauncherForActivityResult(ActivityResultContracts.StartIntentSenderForResult()) {
            model.finishDriveConsent(it.data)
        }
        state.driveConsent?.let { intent ->
            LaunchedEffect(intent) {
                launcher.launch(IntentSenderRequest.Builder(intent).build())
                model.driveConsentShown()
            }
        }
    }

    /**
     * Shows the system's biometric prompt (strong biometrics only) for
     * [cipher]; [done] gets the unlocked cipher, or null when it was
     * cancelled or failed.
     */
    private fun authenticate(title: String, subtitle: String, negative: String, cipher: Cipher, done: (Cipher?) -> Unit) {
        var finished = false
        fun finish(result: Cipher?) {
            if (finished) return
            finished = true
            done(result)
        }
        val prompt = BiometricPrompt(
            this,
            ContextCompat.getMainExecutor(this),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    finish(result.cryptoObject?.cipher)
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    finish(null)
                }
            },
        )
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setSubtitle(subtitle)
            .setNegativeButtonText(negative)
            .setAllowedAuthenticators(BIOMETRIC_STRONG)
            .build()
        try {
            prompt.authenticate(info, BiometricPrompt.CryptoObject(cipher))
        } catch (_: Exception) {
            finish(null)
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

    state.keyToBackUp?.let { KeyBackupDialog(it, model) }

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

/** Right after a key was created here: save its backup, e.g. to Google Drive. */
@Composable
private fun KeyBackupDialog(fingerprint: String, model: VaultViewModel) {
    val save = rememberKeyBackupLauncher(model)
    AlertDialog(
        onDismissRequest = {},
        title = { Text(tr("Save a backup of your key")) },
        text = {
            Text(
                tr(
                    "Without it, losing this phone loses access to your vaults. The backup stays protected by the key's passphrase: keep it somewhere safe, like Google Drive or a computer.",
                ),
            )
        },
        confirmButton = { TextButton(onClick = { save(fingerprint) }) { Text(tr("Save the backup")) } },
        // Translators: button that skips saving the key backup for now.
        dismissButton = { TextButton(onClick = model::skipKeyBackup) { Text(tr("Later")) } },
    )
}

/**
 * A function that asks where to save the backup of a key (any place the
 * system's file picker offers: Google Drive, the phone's storage…), then
 * saves it there.
 */
@Composable
fun rememberKeyBackupLauncher(model: VaultViewModel): (String) -> Unit {
    var saving by rememberSaveable { mutableStateOf<String?>(null) }
    // Not application/pgp-keys: Android's file picker would rename the file to .key.
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val fingerprint = saving
        saving = null
        if (uri != null && fingerprint != null) model.saveKeyBackup(fingerprint, uri)
    }
    return { fingerprint ->
        saving = fingerprint
        launcher.launch(KEY_BACKUP_NAME)
    }
}

/** Same name as the desktop's backup. */
private const val KEY_BACKUP_NAME = "passel-key-backup.asc"

@Composable
private fun PassphraseDialog(request: PromptRequest, onAnswer: (PassphraseAnswer?) -> Unit) {
    var passphrase by remember(request) { mutableStateOf("") }
    var confirm by remember(request) { mutableStateOf("") }
    var useBiometrics by remember(request) { mutableStateOf(false) }
    val creating = request is PromptRequest.NewKey
    val mismatch = creating && passphrase != confirm
    val offerBiometrics = when (request) {
        is PromptRequest.Unlock -> request.offerBiometrics
        is PromptRequest.NewKey -> request.offerBiometrics
        is PromptRequest.Biometric -> false
    }
    AlertDialog(
        onDismissRequest = { onAnswer(null) },
        title = { Text(if (creating) tr("Choose a passphrase") else tr("Unlock your key")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                when (request) {
                    is PromptRequest.Unlock -> {
                        Text(request.key.userId ?: request.key.fingerprint)
                        if (request.retry) Text(tr("Wrong passphrase, try again"), color = MaterialTheme.colorScheme.error)
                        if (request.biometricsFailed) {
                            Text(
                                tr("The passphrase kept for biometric unlock no longer works: enter the key's passphrase."),
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    }
                    is PromptRequest.NewKey -> Text(
                        tr("It protects the key for {user} on this phone and in its backups.", "user" to request.userId),
                    )
                    is PromptRequest.Biometric -> {}
                }
                PasswordField(passphrase, { passphrase = it }, tr("Passphrase"))
                if (creating) PasswordField(confirm, { confirm = it }, tr("Repeat it"))
                if (offerBiometrics) {
                    Row(
                        Modifier.toggleable(value = useBiometrics, onValueChange = { useBiometrics = it }, role = Role.Checkbox),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(checked = useBiometrics, onCheckedChange = null)
                        Text(tr("Unlock with biometrics next time"), Modifier.padding(start = 8.dp))
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onAnswer(PassphraseAnswer(passphrase, useBiometrics)) }, enabled = passphrase.isNotEmpty() && !mismatch) {
                Text(if (creating) tr("Create") else tr("Unlock"))
            }
        },
        dismissButton = { TextButton(onClick = { onAnswer(null) }) { Text(tr("Cancel")) } },
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
