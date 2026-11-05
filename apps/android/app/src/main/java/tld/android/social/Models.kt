package tld.android.social

import kotlinx.serialization.Serializable

@Serializable
data class Counts(
    val events: Long = 0,
    val roots: Long = 0,
    val replies: Long = 0,
    val contacts: Long = 0,
    val verifiedContacts: Long = 0,
    val queuedDeliveries: Long = 0,
    val failedDeliveries: Long = 0,
    val withheldDeliveries: Long = 0,
    val problems: Long = 0,
    val unseenFollowing: Long = 0,
)

@Serializable
data class MailSettings(
    val transport: String = "spool",
    val account: String = "",
    val imapHost: String = "",
    val imapPort: Long = 993,
    val imapTls: String = "implicit",
    val smtpHost: String = "",
    val smtpPort: Long = 587,
    val smtpTls: String = "starttls",
    val username: String = "",
    val passwordEnv: String = "SOCIAL_MAIL_PASSWORD",
    val folder: String = "INBOX",
)

@Serializable
data class Settings(
    val name: String = "",
    val address: String = "",
    val locale: String = "en",
    val transport: String = "spool",
    val account: String = "",
    val imapHost: String = "",
    val imapPort: Long = 993,
    val imapTls: String = "implicit",
    val smtpHost: String = "",
    val smtpPort: Long = 587,
    val smtpTls: String = "starttls",
    val username: String = "",
    val passwordEnv: String = "SOCIAL_MAIL_PASSWORD",
    val folder: String = "INBOX",
    val moveProtocolMessages: Boolean = true,
    val protocolFolder: String = "SocialMail",
    val autoSyncMinutes: Long = 0,
)

@Serializable
data class Status(
    val initialised: Boolean = false,
    val fingerprint: String = "",
    val name: String = "",
    val address: String = "",
    val locale: String = "",
    val createdAt: String = "",
    val counts: Counts = Counts(),
    val mail: MailSettings = MailSettings(),
    val transportError: String? = null,
)

@Serializable
data class FeedItem(
    val eventId: String = "",
    val position: Long = 0,
    val author: String = "",
    val authorName: String = "",
    val trust: String = "unverified",
    val audience: String = "followers",
    val listId: String? = null,
    val text: String = "",
    val createdAt: String = "",
    val receivedAt: String = "",
    val seen: Boolean = true,
    val replyCount: Long = 0,
    val delivery: String? = null,
    val mine: Boolean = false,
)

@Serializable
data class ConversationItem(
    val eventId: String = "",
    val author: String = "",
    val authorName: String = "",
    val trust: String = "unverified",
    val text: String = "",
    val createdAt: String = "",
    val depth: Int = 0,
    val kind: String = "post",
)

@Serializable
data class ContactRow(
    val fingerprint: String = "",
    val name: String = "",
    val address: String = "",
    val trust: String = "unverified",
    val muted: Boolean = false,
    val iFollow: String? = null,
    val followsMe: String? = null,
    val pendingRequest: Boolean = false,
    val addedAt: String = "",
)

@Serializable
data class RequestRow(
    val fingerprint: String = "",
    val name: String = "",
    val address: String = "",
    val updatedAt: String = "",
)

@Serializable
data class ProblemRow(
    val eventId: String = "",
    val author: String? = null,
    val kind: String? = null,
    val reason: String = "",
    val detail: String? = null,
    val createdAt: String = "",
)

@Serializable
data class SyncReport(
    val received: Long = 0,
    val accepted: Long = 0,
    val duplicates: Long = 0,
    val dropped: Long = 0,
    val quarantined: Long = 0,
    val sent: Long = 0,
    val failed: Long = 0,
)

@Serializable
data class DeliveryRow(
    val recipient: String = "",
    val state: String = "",
    val reason: String? = null,
    val attempts: Long = 0,
    val nextAttemptAt: String? = null,
)

@Serializable
data class QueueItem(
    val post: String = "",
    val preview: String = "",
    val deliveries: List<DeliveryRow> = emptyList(),
)

@Serializable
data class CardInfo(
    val fingerprint: String = "",
    val address: String = "",
    val text: String = "",
)

@Serializable
data class ListPayload(val lists: List<String> = emptyList())

@Serializable
data class ImportResult(val outcome: String = "")

@Serializable
data class MailReport(val report: String = "")

@Serializable
data class Fingerprint(val fingerprint: String = "", val name: String = "")
