package tld.android.social.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tld.android.social.AppViewModel
import tld.android.social.ContactRow
import tld.android.social.ConversationItem
import tld.android.social.Destination
import tld.android.social.FeedItem
import tld.android.social.ProblemRow
import tld.android.social.RequestRow
import tld.android.social.Screen
import tld.android.social.UiState

@Composable
fun App(viewModel: AppViewModel) {
    val state = viewModel.state
    val context = LocalContext.current
    LaunchedEffect(Unit) { viewModel.attach(context.filesDir.absolutePath) }

    when {
        !state.booted -> BootScreen()
        !state.initialised -> OnboardingScreen(viewModel)
        else -> MainScaffold(state, viewModel)
    }
}

@Composable
private fun BootScreen() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator()
            Spacer(Modifier.height(16.dp))
            Text("Opening your identity…", style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
private fun OnboardingScreen(viewModel: AppViewModel) {
    var name by remember { mutableStateOf("") }
    var address by remember { mutableStateOf("") }
    var locale by remember { mutableStateOf("en") }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Social Mail", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        Text(
            "A mailbox-shaped social network: no server, no signup. You keep the keys.",
            style = MaterialTheme.typography.bodyMedium,
        )
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = name,
            onValueChange = { name = it },
            label = { Text("Display name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = address,
            onValueChange = { address = it },
            label = { Text("Mail address (optional, for sharing)") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("en" to "English", "fa" to "فارسی").forEach { (key, label) ->
                FilterChip(
                    selected = locale == key,
                    onClick = { locale = key },
                    label = { Text(label) },
                )
            }
        }
        Spacer(Modifier.height(8.dp))
        Button(
            onClick = { viewModel.createIdentity(name, address, locale) },
            enabled = name.isNotBlank(),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Create identity")
        }
        Text(
            "Your private keys are generated on this device and stored in the app's own " +
                "storage, which no other app can read. Losing the device means losing the " +
                "identity; identity restore from a desktop backup is not in this build yet.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.outline,
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MainScaffold(state: UiState, viewModel: AppViewModel) {
    val screen = state.screen
    val title = when (screen) {
        is Screen.Home -> when (state.destination) {
            Destination.Feed -> "Feed"
            Destination.Contacts -> "Contacts"
            Destination.Problems -> "Problems"
            Destination.Settings -> "Settings"
        }
        is Screen.Thread -> "Thread"
        is Screen.Compose -> "New post"
        is Screen.Reply -> "Reply"
        is Screen.Requests -> "Follow requests"
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    if (screen !is Screen.Home) {
                        IconButton(onClick = { viewModel.back() }) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    }
                },
                actions = {
                    IconButton(onClick = { viewModel.sync() }, enabled = !state.busy) {
                        Icon(Icons.Default.Refresh, contentDescription = "Sync")
                    }
                },
            )
        },
        bottomBar = {
            if (screen is Screen.Home) {
                BottomBar(state, viewModel)
            }
        },
        floatingActionButton = {
            if (screen is Screen.Home && state.destination == Destination.Feed) {
                FloatingActionButton(onClick = { viewModel.openScreen(Screen.Compose) }) {
                    Icon(Icons.Default.Edit, contentDescription = "Compose")
                }
            }
        },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize(),
        ) {
            if (state.busy) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            state.status.transportError?.let { problem ->
                Note("Mail transport: $problem", MaterialTheme.colorScheme.errorContainer)
            }
            state.error?.let { message ->
                Note(message, MaterialTheme.colorScheme.errorContainer, onDismiss = { viewModel.clearError() })
            }
            state.message?.let { message ->
                Note(message, MaterialTheme.colorScheme.secondaryContainer, onDismiss = { viewModel.clearMessage() })
            }
            Box(Modifier.weight(1f)) {
                when (screen) {
                    is Screen.Home -> when (state.destination) {
                        Destination.Feed -> FeedScreen(state, viewModel)
                        Destination.Contacts -> ContactsScreen(state, viewModel)
                        Destination.Problems -> ProblemsScreen(state, viewModel)
                        Destination.Settings -> SettingsScreen(state, viewModel)
                    }
                    is Screen.Thread -> ThreadScreen(state, viewModel, screen)
                    is Screen.Compose -> ComposeScreen(state, viewModel)
                    is Screen.Reply -> ReplyScreen(state, viewModel, screen)
                    is Screen.Requests -> RequestsScreen(state, viewModel)
                }
            }
        }
    }
}

@Composable
private fun BottomBar(state: UiState, viewModel: AppViewModel) {
    NavigationBar {
        NavigationBarItem(
            selected = state.destination == Destination.Feed,
            onClick = { viewModel.select(Destination.Feed) },
            icon = { Icon(Icons.Default.Home, contentDescription = "Feed") },
            label = { Text("Feed") },
        )
        NavigationBarItem(
            selected = state.destination == Destination.Contacts,
            onClick = { viewModel.select(Destination.Contacts) },
            icon = { Icon(Icons.Default.Person, contentDescription = "Contacts") },
            label = { Text("Contacts") },
        )
        NavigationBarItem(
            selected = state.destination == Destination.Problems,
            onClick = { viewModel.select(Destination.Problems) },
            icon = { Icon(Icons.Default.Warning, contentDescription = "Problems") },
            label = {
                val count = state.problems.size
                Text(if (count > 0) "Problems ($count)" else "Problems")
            },
        )
        NavigationBarItem(
            selected = state.destination == Destination.Settings,
            onClick = { viewModel.select(Destination.Settings) },
            icon = { Icon(Icons.Default.Settings, contentDescription = "Settings") },
            label = { Text("Settings") },
        )
    }
}

@Composable
fun Note(text: String, container: Color, onDismiss: (() -> Unit)? = null) {
    Surface(color = container, modifier = Modifier.fillMaxWidth()) {
        Row(
            Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
            if (onDismiss != null) {
                TextButton(onClick = onDismiss) { Text("Dismiss") }
            }
        }
    }
}

@Composable
private fun Empty(text: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.outline)
    }
}

// ------------------------------------------------------------------------ feed

@Composable
private fun FeedScreen(state: UiState, viewModel: AppViewModel) {
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            listOf(
                "following" to "Following",
                "lists" to "Lists",
                "you" to "You",
                "all" to "All",
            ).forEach { (key, label) ->
                FilterChip(
                    selected = state.tab == key,
                    onClick = { viewModel.setTab(key) },
                    label = { Text(label) },
                )
            }
        }
        if (state.feed.isEmpty()) {
            Empty("Nothing here yet. Follow someone, or post something.")
        } else {
            LazyColumn(Modifier.fillMaxSize()) {
                items(state.feed, key = { it.eventId }) { item ->
                    FeedCard(item) { viewModel.openThread(item.eventId, item.authorName) }
                }
            }
        }
    }
}

@Composable
private fun FeedCard(item: FeedItem, onClick: () -> Unit) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .clickable(onClick = onClick),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
    ) {
        Column(Modifier.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    if (item.mine) "You" else item.authorName.ifBlank { item.author.take(12) },
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                Spacer(Modifier.width(8.dp))
                TrustDot(item.trust)
                Spacer(Modifier.width(6.dp))
                Text(
                    audienceLabel(item),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.outline,
                )
                Spacer(Modifier.weight(1f))
                Text(
                    shortTime(item.createdAt),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.outline,
                )
            }
            Spacer(Modifier.height(6.dp))
            Text(item.text, style = MaterialTheme.typography.bodyMedium)
            val footer = buildList {
                if (item.replyCount > 0) add("${item.replyCount} replies")
                item.delivery?.let { add(it) }
            }
            if (footer.isNotEmpty()) {
                Spacer(Modifier.height(8.dp))
                Text(
                    footer.joinToString(" · "),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.outline,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun TrustDot(trust: String) {
    val color = when (trust) {
        "verified" -> MaterialTheme.colorScheme.primary
        "blocked" -> MaterialTheme.colorScheme.error
        else -> MaterialTheme.colorScheme.outline
    }
    Box(
        Modifier
            .size(8.dp)
            .background(color, CircleShape),
    )
}

private fun audienceLabel(item: FeedItem): String = when (item.audience) {
    "public-list" -> "public list"
    else -> "followers"
}

fun shortTime(value: String): String {
    if (value.length >= 16) {
        return value.substring(0, 16).replace('T', ' ')
    }
    return value
}

// ---------------------------------------------------------------------- thread

@Composable
private fun ThreadScreen(state: UiState, viewModel: AppViewModel, screen: Screen.Thread) {
    Column(Modifier.fillMaxSize()) {
        if (state.thread.isEmpty()) {
            Box(Modifier.weight(1f)) { Empty("No replies yet.") }
        } else {
            LazyColumn(Modifier.weight(1f)) {
                items(state.thread, key = { it.eventId }) { item ->
                    ThreadRow(item)
                }
            }
        }
        HorizontalDivider()
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedButton(
                onClick = { viewModel.openScreen(Screen.Reply(screen.root)) },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(Icons.AutoMirrored.Filled.Send, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text("Reply")
            }
        }
    }
}

@Composable
private fun ThreadRow(item: ConversationItem) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = (12 + item.depth * 16).dp, end = 12.dp, top = 6.dp, bottom = 6.dp),
        shape = MaterialTheme.shapes.small,
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
    ) {
        Column(Modifier.padding(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    item.authorName.ifBlank { item.author.take(12) },
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                Spacer(Modifier.width(6.dp))
                TrustDot(item.trust)
                Spacer(Modifier.weight(1f))
                Text(
                    shortTime(item.createdAt),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.outline,
                )
            }
            Spacer(Modifier.height(4.dp))
            Text(item.text, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

// --------------------------------------------------------------------- compose

@Composable
private fun ComposeScreen(state: UiState, viewModel: AppViewModel) {
    var text by remember { mutableStateOf("") }
    var audience by remember { mutableStateOf("followers") }
    var list by remember { mutableStateOf(state.lists.firstOrNull() ?: "") }

    Column(
        Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(
                selected = audience == "followers",
                onClick = { audience = "followers" },
                label = { Text("Followers") },
            )
            FilterChip(
                selected = audience == "public-list",
                onClick = { audience = "public-list" },
                label = { Text("Public list") },
            )
        }
        if (audience == "public-list") {
            OutlinedTextField(
                value = list,
                onValueChange = { list = it },
                label = { Text("List address") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        OutlinedTextField(
            value = text,
            onValueChange = { text = it },
            label = { Text("What do you want to say?") },
            modifier = Modifier
                .fillMaxWidth()
                .height(180.dp),
        )
        Text(
            if (audience == "followers") {
                "Encrypted for each verified follower. Unverified followers are recorded as withheld."
            } else {
                "Signed, not encrypted. The list operator relays it to subscribers."
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.outline,
        )
        Button(
            onClick = {
                viewModel.post(text, audience, if (audience == "public-list") list else null)
            },
            enabled = text.isNotBlank() && !state.busy &&
                (audience != "public-list" || list.isNotBlank()),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Post")
        }
    }
}

@Composable
private fun ReplyScreen(state: UiState, viewModel: AppViewModel, screen: Screen.Reply) {
    var text by remember { mutableStateOf("") }
    Column(
        Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        OutlinedTextField(
            value = text,
            onValueChange = { text = it },
            label = { Text("Reply") },
            modifier = Modifier
                .fillMaxWidth()
                .height(140.dp),
        )
        Button(
            onClick = { viewModel.reply(screen.parent, text) },
            enabled = text.isNotBlank() && !state.busy,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Send reply")
        }
    }
}

// -------------------------------------------------------------------- requests

@Composable
private fun RequestsScreen(state: UiState, viewModel: AppViewModel) {
    if (state.requests.isEmpty()) {
        Empty("No pending follow requests.")
        return
    }
    LazyColumn(Modifier.fillMaxSize()) {
        items(state.requests, key = { it.fingerprint }) { request ->
            RequestCard(request, viewModel)
        }
    }
}

@Composable
private fun RequestCard(request: RequestRow, viewModel: AppViewModel) {
    var verify by remember(request.fingerprint) { mutableStateOf(true) }
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
    ) {
        Column(Modifier.padding(14.dp)) {
            Text(
                request.name.ifBlank { request.fingerprint.take(16) },
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            if (request.address.isNotBlank()) {
                Text(
                    request.address,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.outline,
                )
            }
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = { verify = !verify }) {
                    Text(if (verify) "✓ Verify this key" else "Accept without verifying")
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { viewModel.approve(request.fingerprint, verify) }) {
                    Text("Accept")
                }
                OutlinedButton(onClick = { viewModel.decline(request.fingerprint) }) {
                    Text("Decline")
                }
            }
        }
    }
}

@Composable
fun ProblemCard(problem: ProblemRow, viewModel: AppViewModel) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
    ) {
        Column(Modifier.padding(14.dp)) {
            Text(
                problem.reason.replace('-', ' '),
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            problem.detail?.let {
                Spacer(Modifier.height(4.dp))
                Text(it, style = MaterialTheme.typography.bodySmall)
            }
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    shortTime(problem.createdAt),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.outline,
                )
                Spacer(Modifier.weight(1f))
                TextButton(onClick = { viewModel.dismissProblem(problem.eventId) }) {
                    Text("Dismiss")
                }
            }
        }
    }
}

@Composable
fun ContactCardRow(contact: ContactRow, viewModel: AppViewModel) {
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
    ) {
        Column(Modifier.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TrustDot(contact.trust)
                Spacer(Modifier.width(8.dp))
                Text(
                    contact.name.ifBlank { contact.fingerprint.take(16) },
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                Spacer(Modifier.weight(1f))
                if (contact.muted) {
                    Text("muted", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.outline)
                }
            }
            if (contact.address.isNotBlank()) {
                Text(
                    contact.address,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.outline,
                )
            }
            Text(
                contact.fingerprint.take(24) + "…",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.outline,
            )
            Spacer(Modifier.height(6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                if (contact.trust != "verified") {
                    TextButton(onClick = { viewModel.verifyContact(contact.fingerprint) }) { Text("Verify") }
                }
                TextButton(onClick = { viewModel.mute(contact.fingerprint, !contact.muted) }) {
                    Text(if (contact.muted) "Unmute" else "Mute")
                }
                TextButton(onClick = { viewModel.block(contact.fingerprint, contact.trust != "blocked") }) {
                    Text(if (contact.trust == "blocked") "Unblock" else "Block")
                }
            }
        }
    }
}
