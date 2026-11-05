package tld.android.social.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import tld.android.social.AppViewModel
import tld.android.social.BuildConfig
import tld.android.social.Screen
import tld.android.social.Settings
import tld.android.social.UiState

// -------------------------------------------------------------------- contacts

@Composable
fun ContactsScreen(state: UiState, viewModel: AppViewModel) {
    var addOpen by remember { mutableStateOf(false) }
    var cardOpen by remember { mutableStateOf(false) }

    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            OutlinedButton(onClick = { addOpen = true }) { Text("Add contact") }
            OutlinedButton(onClick = {
                viewModel.loadCard()
                cardOpen = true
            }) { Text("My card") }
        }

        if (state.requests.isNotEmpty()) {
            Surface(
                color = MaterialTheme.colorScheme.secondaryContainer,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp)
                    .clickable { viewModel.openScreen(Screen.Requests) },
            ) {
                Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "${state.requests.size} follow request(s) waiting",
                        Modifier.weight(1f),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Text("Review", style = MaterialTheme.typography.labelLarge)
                }
            }
            Spacer(Modifier.height(8.dp))
        }

        if (state.contacts.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(
                    "No contacts yet. Paste someone's card to add them.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.outline,
                )
            }
        } else {
            LazyColumn(Modifier.fillMaxSize()) {
                items(state.contacts, key = { it.fingerprint }) { contact ->
                    ContactCardRow(contact, viewModel)
                }
            }
        }
    }

    if (addOpen) {
        AddContactDialog(
            onDismiss = { addOpen = false },
            onAdd = { text ->
                viewModel.importCard(text)
                addOpen = false
            },
        )
    }
    if (cardOpen) {
        CardDialog(
            text = state.card?.text,
            onDismiss = {
                cardOpen = false
                viewModel.clearCard()
            },
        )
    }
}

@Composable
private fun AddContactDialog(onDismiss: () -> Unit, onAdd: (String) -> Unit) {
    var text by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Add a contact") },
        text = {
            Column {
                Text(
                    "Paste a contact card you received from someone.",
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text("Contact card") },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(160.dp),
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onAdd(text) }, enabled = text.isNotBlank()) { Text("Add") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun CardDialog(text: String?, onDismiss: () -> Unit) {
    val clipboard = LocalClipboardManager.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Your contact card") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                Text(
                    "Share this with someone so they can follow you.",
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(8.dp))
                Text(text ?: "Loading…", style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    if (!text.isNullOrBlank()) {
                        clipboard.setText(AnnotatedString(text))
                    }
                },
                enabled = !text.isNullOrBlank(),
            ) { Text("Copy") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}

// -------------------------------------------------------------------- problems

@Composable
fun ProblemsScreen(state: UiState, viewModel: AppViewModel) {
    if (state.problems.isEmpty()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text(
                "Nothing needs attention.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.outline,
            )
        }
        return
    }
    Column(Modifier.fillMaxSize()) {
        Text(
            "Messages that could not be trusted or delivered are kept here, never hidden.",
            style = MaterialTheme.typography.bodySmall,
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
            color = MaterialTheme.colorScheme.outline,
        )
        LazyColumn(Modifier.fillMaxSize()) {
            items(state.problems, key = { it.eventId }) { problem ->
                ProblemCard(problem, viewModel)
            }
        }
    }
}

// -------------------------------------------------------------------- settings

@Composable
fun SettingsScreen(state: UiState, viewModel: AppViewModel) {
    var draft by remember(state.settings) { mutableStateOf(state.settings) }
    var password by remember { mutableStateOf(state.password) }
    var newList by remember { mutableStateOf("") }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("Identity", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Text(
            state.status.fingerprint,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.outline,
        )
        Field("Display name", draft.name) { draft = draft.copy(name = it) }
        Field("Mail address", draft.address) { draft = draft.copy(address = it) }

        Spacer(Modifier.height(8.dp))
        HorizontalDivider()
        Text("Transport", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(
                selected = draft.transport == "spool",
                onClick = { draft = draft.copy(transport = "spool") },
                label = { Text("Spool") },
            )
            FilterChip(
                selected = draft.transport == "mail",
                onClick = { draft = draft.copy(transport = "mail") },
                label = { Text("Mailbox") },
            )
        }
        Text(
            if (draft.transport == "mail") {
                "Sends over SMTP, reads over IMAP. TLS is always verified."
            } else {
                "Writes carriers to a shared folder. Useful for demos only."
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.outline,
        )

        Spacer(Modifier.height(8.dp))
        HorizontalDivider()
        Text("Mailbox", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Field("Account (envelope sender)", draft.account) { draft = draft.copy(account = it) }
        Field("Login name (if different)", draft.username) { draft = draft.copy(username = it) }
        Field("IMAP host", draft.imapHost) { draft = draft.copy(imapHost = it) }
        Field("IMAP port", draft.imapPort.toString(), numeric = true) { value ->
            value.toLongOrNull()?.let { draft = draft.copy(imapPort = it) }
        }
        TlsPicker(draft.imapTls) { draft = draft.copy(imapTls = it) }
        Field("SMTP host", draft.smtpHost) { draft = draft.copy(smtpHost = it) }
        Field("SMTP port", draft.smtpPort.toString(), numeric = true) { value ->
            value.toLongOrNull()?.let { draft = draft.copy(smtpPort = it) }
        }
        TlsPicker(draft.smtpTls) { draft = draft.copy(smtpTls = it) }
        Field("Folder to read", draft.folder) { draft = draft.copy(folder = it) }
        OutlinedTextField(
            value = password,
            onValueChange = {
                password = it
                viewModel.setPassword(it)
            },
            label = { Text("App password (this session only)") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            "The password is never stored: it is passed to the engine for this session " +
                "and forgotten when the app closes.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.outline,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { viewModel.saveSettings(draft) }, enabled = !state.busy) {
                Text("Save")
            }
            OutlinedButton(onClick = { viewModel.mailTest(false) }, enabled = !state.busy) {
                Text("Test sign-in")
            }
        }
        OutlinedButton(
            onClick = { viewModel.mailTest(true) },
            enabled = !state.busy && draft.transport == "mail",
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Send a test message to myself")
        }

        Spacer(Modifier.height(8.dp))
        HorizontalDivider()
        Text("Sync", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Button(onClick = { viewModel.sync() }, enabled = !state.busy) { Text("Sync now") }
            Text(
                "${state.status.counts.events} events · ${state.status.counts.contacts} contacts",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.outline,
            )
        }

        if (state.queue.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            HorizontalDivider()
            Text("Pending deliveries", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            state.queue.forEach { item ->
                Column(Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
                    Text(item.preview, style = MaterialTheme.typography.bodySmall)
                    item.deliveries.forEach { delivery ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                "${delivery.recipient.take(12)}… ${delivery.state}",
                                Modifier.weight(1f),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.outline,
                            )
                            TextButton(onClick = { viewModel.retryDelivery(item.post, delivery.recipient) }) {
                                Text("Retry")
                            }
                            TextButton(onClick = { viewModel.cancelDelivery(item.post, delivery.recipient) }) {
                                Text("Cancel")
                            }
                        }
                    }
                }
            }
        }

        Spacer(Modifier.height(8.dp))
        HorizontalDivider()
        Text("Lists", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        state.lists.forEach { address ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(address, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
                TextButton(onClick = { viewModel.removeList(address) }) { Text("Remove") }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = newList,
                onValueChange = { newList = it },
                label = { Text("List address") },
                singleLine = true,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            TextButton(
                onClick = {
                    viewModel.addList(newList)
                    newList = ""
                },
                enabled = newList.isNotBlank(),
            ) { Text("Add") }
        }

        Spacer(Modifier.height(16.dp))
        HorizontalDivider()
        Text(
            "Social Mail ${BuildConfig.VERSION_NAME}",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.outline,
        )
        Text(
            "Delivery states describe what the transport did, never what a reader did. " +
                "\"Accepted for delivery\" means your mail server took the message.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.outline,
        )
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun TlsPicker(current: String, onPick: (String) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        listOf("implicit" to "Implicit TLS", "starttls" to "STARTTLS", "none" to "None").forEach { (key, label) ->
            FilterChip(
                selected = current == key,
                onClick = { onPick(key) },
                label = { Text(label, style = MaterialTheme.typography.labelSmall) },
            )
        }
    }
}

@Composable
fun Field(
    label: String,
    value: String,
    numeric: Boolean = false,
    secret: Boolean = false,
    onChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = if (numeric) {
            KeyboardOptions(keyboardType = KeyboardType.Number)
        } else {
            KeyboardOptions.Default
        },
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        modifier = Modifier.fillMaxWidth(),
    )
}

@Suppress("unused")
private fun Settings.densityLabel(): String = if (transport == "mail") "mailbox" else "spool"
