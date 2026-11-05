package tld.android.social

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/**
 * The single native entry point: one JSON request in, one JSON answer out.
 * See `crates/android-jni/src/lib.rs` for the other half of this contract.
 */
class Core private constructor() {
    companion object {
        init {
            System.loadLibrary("social_mail_android")
        }

        @JvmStatic
        external fun call(request: String): String?
    }
}

class CoreException(message: String) : Exception(message)

object Engine {
    val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        isLenient = true
        encodeDefaults = true
    }

    fun call(request: JsonObject): JsonElement {
        val raw = Core.call(request.toString()) ?: throw CoreException("the engine returned nothing")
        val envelope = json.parseToJsonElement(raw).jsonObject
        val ok = envelope["ok"]?.jsonPrimitive?.booleanOrNull ?: false
        if (!ok) {
            throw CoreException(envelope["error"]?.jsonPrimitive?.contentOrNull ?: "the engine failed")
        }
        return envelope["data"] ?: JsonNull
    }

    inline fun <reified T> decode(element: JsonElement): T = json.decodeFromJsonElement<T>(element)
}

/** Builds a request document. `home` is the app's private directory. */
object Request {
    fun of(
        home: String,
        cmd: String,
        password: String? = null,
        extra: JsonObjectBuilder.() -> Unit = {},
    ): JsonObject = buildJsonObject {
        put("cmd", cmd)
        put("home", home)
        if (!password.isNullOrEmpty()) {
            put("password", password)
        }
        extra()
    }
}
