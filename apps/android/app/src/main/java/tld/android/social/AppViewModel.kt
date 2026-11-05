package tld.android.social

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

enum class Destination { Feed, Contacts, Problems, Settings }

sealed interface Screen {
    data object Home : Screen
    data class Thread(val root: String, val title: String) : Screen
    data object Compose : Screen
    data class Reply(val parent: String) : Screen
    data object Requests : Screen
}

data class UiState(
    val booted: Boolean = false,
    val initialised: Boolean = false,
    val busy: Boolean = false,
    val destination: Destination = Destination.Feed,
    val screen: Screen = Screen.Home,
    val tab: String = "following",
    val status: Status = Status(),
    val feed: List<FeedItem> = emptyList(),
    val thread: List<ConversationItem> = emptyList(),
    val contacts: List<ContactRow> = emptyList(),
    val requests: List<RequestRow> = emptyList(),
    val problems: List<ProblemRow> = emptyList(),
    val queue: List<QueueItem> = emptyList(),
    val lists: List<String> = emptyList(),
    val settings: Settings = Settings(),
    val card: CardInfo? = null,
    val password: String = "",
    val message: String? = null,
    val error: String? = null,
)

/**
 * Everything the app does goes through the engine. The password lives in this
 * object for the session only: it is never written to preferences, files or the
 * database, and it is passed to the bridge per request.
 */
class AppViewModel : ViewModel() {

    var state by mutableStateOf(UiState())
        private set

    private var home: String = ""
    private val gate = Mutex()

    // ------------------------------------------------------------------ plumbing

    fun attach(path: String) {
        if (home.isNotEmpty()) return
        home = path
        launch { reload() }
    }

    private fun launch(block: suspend () -> Unit) {
        viewModelScope.launch {
            gate.withLock {
                state = state.copy(busy = true)
                try {
                    withContext(Dispatchers.IO) { block() }
                } catch (err: CoreException) {
                    state = state.copy(error = err.message)
                } catch (err: Exception) {
                    state = state.copy(error = err.message ?: err.toString())
                } finally {
                    state = state.copy(busy = false)
                }
            }
        }
    }

    private suspend fun reload() {
        val status: Status = Engine.decode(Engine.call(Request.of(home, "status")))
        if (!status.initialised) {
            state = state.copy(booted = true, initialised = false, status = status)
            return
        }
        val settings: Settings = Engine.decode(Engine.call(Request.of(home, "settings")))
        val feed: List<FeedItem> = Engine.decode(Engine.call(Request.of(home, "feed") {
            put("tab", state.tab)
            put("limit", 100)
        }))
        val contacts: List<ContactRow> = Engine.decode(Engine.call(Request.of(home, "contacts")))
        val requests: List<RequestRow> = Engine.decode(Engine.call(Request.of(home, "requests")))
        val problems: List<ProblemRow> = Engine.decode(Engine.call(Request.of(home, "problems")))
        val lists: ListPayload = Engine.decode(Engine.call(Request.of(home, "lists")))
        val queue: List<QueueItem> = Engine.decode(Engine.call(Request.of(home, "queue") {
            put("limit", 50)
        }))
        state = state.copy(
            booted = true,
            initialised = true,
            status = status,
            settings = settings,
            feed = feed,
            contacts = contacts,
            requests = requests,
            problems = problems,
            lists = lists.lists,
            queue = queue,
        )
    }

    private suspend fun reloadInbox() {
        val status: Status = Engine.decode(Engine.call(Request.of(home, "status")))
        val feed: List<FeedItem> = Engine.decode(Engine.call(Request.of(home, "feed") {
            put("tab", state.tab)
            put("limit", 100)
        }))
        val problems: List<ProblemRow> = Engine.decode(Engine.call(Request.of(home, "problems")))
        val requests: List<RequestRow> = Engine.decode(Engine.call(Request.of(home, "requests")))
        val queue: List<QueueItem> = Engine.decode(Engine.call(Request.of(home, "queue") {
            put("limit", 50)
        }))
        state = state.copy(
            status = status,
            feed = feed,
            problems = problems,
            requests = requests,
            queue = queue,
        )
    }

    private fun password(): String = state.password

    // --------------------------------------------------------------------- views

    fun select(destination: Destination) {
        state = state.copy(destination = destination, screen = Screen.Home)
        launch {
            when (destination) {
                Destination.Contacts -> {
                    val contacts: List<ContactRow> =
                        Engine.decode(Engine.call(Request.of(home, "contacts")))
                    val requests: List<RequestRow> =
                        Engine.decode(Engine.call(Request.of(home, "requests")))
                    state = state.copy(contacts = contacts, requests = requests)
                }
                Destination.Problems -> {
                    val problems: List<ProblemRow> =
                        Engine.decode(Engine.call(Request.of(home, "problems")))
                    state = state.copy(problems = problems)
                }
                Destination.Settings -> {
                    val settings: Settings =
                        Engine.decode(Engine.call(Request.of(home, "settings")))
                    val lists: ListPayload = Engine.decode(Engine.call(Request.of(home, "lists")))
                    val queue: List<QueueItem> = Engine.decode(Engine.call(Request.of(home, "queue") {
                        put("limit", 50)
                    }))
                    state = state.copy(settings = settings, lists = lists.lists, queue = queue)
                }
                Destination.Feed -> {
                    val feed: List<FeedItem> = Engine.decode(Engine.call(Request.of(home, "feed") {
                        put("tab", state.tab)
                        put("limit", 100)
                    }))
                    state = state.copy(feed = feed)
                }
            }
        }
    }

    fun openScreen(screen: Screen) {
        state = state.copy(screen = screen)
    }

    fun back() {
        state = state.copy(screen = Screen.Home)
    }

    fun setTab(tab: String) {
        state = state.copy(tab = tab)
        launch {
            val feed: List<FeedItem> = Engine.decode(Engine.call(Request.of(home, "feed") {
                put("tab", tab)
                put("limit", 100)
            }))
            state = state.copy(feed = feed)
        }
    }

    fun setPassword(value: String) {
        state = state.copy(password = value)
    }

    fun clearError() {
        state = state.copy(error = null)
    }

    fun clearMessage() {
        state = state.copy(message = null)
    }

    // -------------------------------------------------------------------- actions

    fun refresh() = launch { reloadInbox() }

    fun createIdentity(name: String, address: String, locale: String) {
        launch {
            val result: Fingerprint = Engine.decode(Engine.call(Request.of(home, "create") {
                put("name", name)
                put("address", address)
                put("locale", locale)
            }))
            val status: Status = Engine.decode(Engine.call(Request.of(home, "status")))
            state = state.copy(
                initialised = true,
                status = status,
                booted = true,
                message = "Identity created: ${result.fingerprint.take(12)}…",
            )
            reload()
        }
    }

    fun sync() {
        launch {
            val report: SyncReport = Engine.decode(Engine.call(Request.of(home, "sync", password())))
            reloadInbox()
            state = state.copy(
                message = if (report.received == 0L && report.sent == 0L) {
                    "Nothing to sync."
                } else {
                    "Sync: ${report.received} received, ${report.accepted} accepted, " +
                        "${report.quarantined} quarantined, ${report.sent} accepted for delivery."
                },
            )
        }
    }

    fun post(text: String, audience: String, list: String?) {
        launch {
            Engine.call(Request.of(home, "post", password()) {
                put("text", text)
                put("audience", audience)
                if (!list.isNullOrBlank()) put("list", list)
            })
            state = state.copy(message = "Posted. Queued for your followers.")
            reloadInbox()
            state = state.copy(screen = Screen.Home)
        }
    }

    fun reply(parent: String, text: String) {
        launch {
            Engine.call(Request.of(home, "reply", password()) {
                put("parent", parent)
                put("text", text)
            })
            val thread: List<ConversationItem> = Engine.decode(Engine.call(Request.of(home, "conversation") {
                put("root", parent)
            }))
            state = state.copy(thread = thread, message = "Reply queued.", screen = Screen.Thread(parent, ""))
            reloadInbox()
        }
    }

    fun openThread(root: String, title: String) {
        state = state.copy(screen = Screen.Thread(root, title))
        launch {
            val thread: List<ConversationItem> = Engine.decode(Engine.call(Request.of(home, "conversation") {
                put("root", root)
            }))
            state = state.copy(thread = thread)
        }
    }

    fun follow(target: String) {
        launch {
            Engine.call(Request.of(home, "follow", password()) { put("target", target) })
            val contacts: List<ContactRow> = Engine.decode(Engine.call(Request.of(home, "contacts")))
            state = state.copy(contacts = contacts, message = "Follow request queued.")
            reloadInbox()
        }
    }

    fun importCard(text: String) {
        launch {
            val result: ImportResult = Engine.decode(Engine.call(Request.of(home, "import_card") {
                put("text", text)
            }))
            val message = when (result.outcome) {
                "added" -> "Contact added. You can follow them now."
                "updated" -> "Contact card updated."
                "unchanged" -> "This card is already known; nothing changed."
                "mismatch" -> "A different key claims a known address. Not imported."
                "own-card" -> "That is your own card."
                else -> "Card import: ${result.outcome}"
            }
            val contacts: List<ContactRow> = Engine.decode(Engine.call(Request.of(home, "contacts")))
            state = state.copy(contacts = contacts, message = message)
        }
    }

    fun loadCard() {
        launch {
            val card: CardInfo = Engine.decode(Engine.call(Request.of(home, "card")))
            state = state.copy(card = card)
        }
    }

    fun approve(fingerprint: String, verify: Boolean) {
        launch {
            Engine.call(Request.of(home, "approve", password()) {
                put("fingerprint", fingerprint)
                put("verify", verify)
            })
            val requests: List<RequestRow> = Engine.decode(Engine.call(Request.of(home, "requests")))
            val contacts: List<ContactRow> = Engine.decode(Engine.call(Request.of(home, "contacts")))
            state = state.copy(
                requests = requests,
                contacts = contacts,
                message = if (verify) "Accepted as a verified follower." else "Accepted as a follower.",
            )
        }
    }

    fun decline(fingerprint: String) {
        launch {
            Engine.call(Request.of(home, "decline", password()) { put("fingerprint", fingerprint) })
            val requests: List<RequestRow> = Engine.decode(Engine.call(Request.of(home, "requests")))
            state = state.copy(requests = requests, message = "Request declined.")
        }
    }

    fun verifyContact(fingerprint: String) {
        launch {
            Engine.call(Request.of(home, "verify") { put("fingerprint", fingerprint) })
            val contacts: List<ContactRow> = Engine.decode(Engine.call(Request.of(home, "contacts")))
            state = state.copy(contacts = contacts, message = "Marked as verified.")
        }
    }

    fun block(fingerprint: String, blocked: Boolean) {
        launch {
            val command = if (blocked) "block" else "unblock"
            Engine.call(Request.of(home, command) { put("fingerprint", fingerprint) })
            val contacts: List<ContactRow> = Engine.decode(Engine.call(Request.of(home, "contacts")))
            state = state.copy(contacts = contacts, message = if (blocked) "Blocked." else "Unblocked.")
        }
    }

    fun mute(fingerprint: String, muted: Boolean) {
        launch {
            Engine.call(Request.of(home, "mute") {
                put("fingerprint", fingerprint)
                put("muted", muted)
            })
            val contacts: List<ContactRow> = Engine.decode(Engine.call(Request.of(home, "contacts")))
            state = state.copy(contacts = contacts, message = if (muted) "Muted." else "Unmuted.")
        }
    }

    fun dismissProblem(eventId: String) {
        launch {
            Engine.call(Request.of(home, "dismiss_problem") { put("eventId", eventId) })
            val problems: List<ProblemRow> = Engine.decode(Engine.call(Request.of(home, "problems")))
            state = state.copy(problems = problems)
            reloadInbox()
        }
    }

    fun saveSettings(draft: Settings) {
        launch {
            val saved: Settings = Engine.decode(Engine.call(Request.of(home, "settings_set", password()) {
                put("name", draft.name)
                put("address", draft.address)
                put("locale", draft.locale)
                put("transport", draft.transport)
                put("account", draft.account)
                put("imapHost", draft.imapHost)
                put("imapPort", draft.imapPort)
                put("imapTls", draft.imapTls)
                put("smtpHost", draft.smtpHost)
                put("smtpPort", draft.smtpPort)
                put("smtpTls", draft.smtpTls)
                put("username", draft.username)
                put("passwordEnv", draft.passwordEnv)
                put("folder", draft.folder)
                put("protocolFolder", draft.protocolFolder)
                put("moveProtocolMessages", draft.moveProtocolMessages)
                put("autoSyncMinutes", draft.autoSyncMinutes)
            }))
            state = state.copy(settings = saved, message = "Settings saved.")
            reloadInbox()
        }
    }

    fun mailTest(send: Boolean) {
        launch {
            val report: MailReport = Engine.decode(Engine.call(Request.of(home, "mail_test", password()) {
                put("send", send)
            }))
            state = state.copy(message = report.report)
        }
    }

    fun addList(address: String) {
        launch {
            Engine.call(Request.of(home, "add_list") { put("address", address) })
            val lists: ListPayload = Engine.decode(Engine.call(Request.of(home, "lists")))
            state = state.copy(lists = lists.lists, message = "List added.")
        }
    }

    fun removeList(address: String) {
        launch {
            Engine.call(Request.of(home, "remove_list") { put("address", address) })
            val lists: ListPayload = Engine.decode(Engine.call(Request.of(home, "lists")))
            state = state.copy(lists = lists.lists, message = "List removed.")
        }
    }

    fun retryDelivery(post: String, recipient: String) {
        launch {
            Engine.call(Request.of(home, "retry_delivery") {
                put("post", post)
                put("recipient", recipient)
            })
            val queue: List<QueueItem> = Engine.decode(Engine.call(Request.of(home, "queue") {
                put("limit", 50)
            }))
            state = state.copy(queue = queue, message = "Delivery queued again.")
        }
    }

    fun cancelDelivery(post: String, recipient: String) {
        launch {
            Engine.call(Request.of(home, "cancel_delivery") {
                put("post", post)
                put("recipient", recipient)
            })
            val queue: List<QueueItem> = Engine.decode(Engine.call(Request.of(home, "queue") {
                put("limit", 50)
            }))
            state = state.copy(queue = queue, message = "Delivery cancelled.")
        }
    }

    fun clearCard() {
        state = state.copy(card = null)
    }
}
