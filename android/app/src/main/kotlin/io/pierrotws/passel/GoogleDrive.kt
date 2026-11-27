package io.pierrotws.passel

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.ClearTokenRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Task
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONObject

/** A vault file on Google Drive. */
data class DriveFile(val id: String, val name: String, val version: String, val modifiedTime: String?) {
    /** `Passwords.tar.xz` → `Passwords`. */
    val vaultName: String get() = name.removeSuffix(DriveClient.SUFFIX)
}

/** A Drive request failed with an HTTP [code]. */
class DriveException(val code: Int, message: String) : IOException(message)

/**
 * Google sign-in for Drive, with the `drive.file` scope: Passel sees only
 * the files it made, never the rest of the user's Drive.
 */
class DriveAuth(private val context: Context) {
    sealed interface Result {
        data class Token(val token: String) : Result

        /** The user must choose an account or allow access: launch [intent]. */
        data class NeedsConsent(val intent: PendingIntent) : Result
    }

    private val client get() = Identity.getAuthorizationClient(context)

    private val request = AuthorizationRequest.builder()
        .setRequestedScopes(listOf(Scope(DRIVE_FILE_SCOPE)))
        .build()

    /** A token right away when access was given before, else what to show. */
    suspend fun authorize(): Result {
        val result = client.authorize(request).await()
        val token = result.accessToken
        return when {
            result.hasResolution() -> Result.NeedsConsent(result.pendingIntent!!)
            token != null -> Result.Token(token)
            else -> throw IOException("no access token")
        }
    }

    /** The token from the consent screen's result, or null if it was refused. */
    fun tokenFromConsent(data: Intent?): String? =
        data?.let { runCatching { client.getAuthorizationResultFromIntent(it).accessToken }.getOrNull() }

    /** Forgets an expired token, so the next [authorize] gets a new one. */
    suspend fun clearToken(token: String) {
        runCatching { client.clearToken(ClearTokenRequest.builder().setToken(token).build()).await() }
    }

    companion object {
        const val DRIVE_FILE_SCOPE = "https://www.googleapis.com/auth/drive.file"
    }
}

private suspend fun <T> Task<T>.await(): T = suspendCancellableCoroutine { continuation ->
    addOnSuccessListener { continuation.resume(it) }
    addOnFailureListener { continuation.resumeWithException(it) }
    addOnCanceledListener { continuation.cancel() }
}

/**
 * The few calls of Drive's REST API (v3) that Passel needs: each vault is
 * one `.tar.xz` file, tagged with `appProperties` so it can be found
 * again, in a "Passel" folder. Every call blocks.
 */
class DriveClient(private val token: String) {
    /** The vaults on this Drive (that Passel made), newest first. */
    fun vaults(): List<DriveFile> {
        val query = "appProperties has { key='$TAG_KEY' and value='$TAG_VAULT' } and trashed=false"
        val json = request(
            "GET",
            "$API/files?q=${encode(query)}&spaces=drive&orderBy=modifiedTime%20desc&pageSize=100" +
                "&fields=${encode("files($FIELDS)")}",
        )
        val files = json.getJSONArray("files")
        return (0 until files.length()).map { file(files.getJSONObject(it)) }
    }

    /** The file's current state; null when it was deleted or trashed. */
    fun file(id: String): DriveFile? = try {
        val json = request("GET", "$API/files/${encode(id)}?fields=${encode("$FIELDS,trashed")}")
        if (json.optBoolean("trashed")) null else file(json)
    } catch (e: DriveException) {
        if (e.code == HttpURLConnection.HTTP_NOT_FOUND) null else throw e
    }

    fun download(id: String, to: File) {
        val connection = open("GET", "$API/files/${encode(id)}?alt=media")
        check(connection)
        connection.inputStream.use { input -> to.outputStream().use { input.copyTo(it) } }
    }

    /** Uploads a new vault file named after [vaultName]. */
    fun create(vaultName: String, from: File): DriveFile {
        val metadata = JSONObject()
            .put("name", vaultName + SUFFIX)
            .put("mimeType", MIME_TYPE)
            .put("parents", listOf(folder()).toJsonArray())
            .put("appProperties", JSONObject().put(TAG_KEY, TAG_VAULT))
        return file(multipart("POST", "$UPLOAD/files?uploadType=multipart&fields=${encode(FIELDS)}", metadata, from))
    }

    /** Replaces the contents of vault file [id]. */
    fun update(id: String, from: File): DriveFile {
        val connection = open("PATCH", "$UPLOAD/files/${encode(id)}?uploadType=media&fields=${encode(FIELDS)}")
        connection.doOutput = true
        connection.setRequestProperty("Content-Type", MIME_TYPE)
        connection.setFixedLengthStreamingMode(from.length())
        connection.outputStream.use { out -> from.inputStream().use { it.copyTo(out) } }
        return file(read(connection))
    }

    /** The "Passel" folder at the top of the Drive, made if missing. */
    private fun folder(): String {
        val query = "appProperties has { key='$TAG_KEY' and value='$TAG_FOLDER' } and trashed=false"
        val found = request("GET", "$API/files?q=${encode(query)}&spaces=drive&fields=${encode("files(id)")}").getJSONArray("files")
        if (found.length() > 0) return found.getJSONObject(0).getString("id")
        val metadata = JSONObject()
            .put("name", "Passel")
            .put("mimeType", "application/vnd.google-apps.folder")
            .put("appProperties", JSONObject().put(TAG_KEY, TAG_FOLDER))
        return request("POST", "$API/files?fields=id", metadata).getString("id")
    }

    private fun multipart(method: String, url: String, metadata: JSONObject, file: File): JSONObject {
        val boundary = "passel-" + java.util.UUID.randomUUID()
        val head = "--$boundary\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n$metadata\r\n" +
            "--$boundary\r\nContent-Type: $MIME_TYPE\r\n\r\n"
        val tail = "\r\n--$boundary--\r\n"
        val connection = open(method, url)
        connection.doOutput = true
        connection.setRequestProperty("Content-Type", "multipart/related; boundary=$boundary")
        val headBytes = head.toByteArray()
        val tailBytes = tail.toByteArray()
        connection.setFixedLengthStreamingMode(headBytes.size + file.length() + tailBytes.size)
        connection.outputStream.use { out ->
            out.write(headBytes)
            file.inputStream().use { it.copyTo(out) }
            out.write(tailBytes)
        }
        return read(connection)
    }

    private fun request(method: String, url: String, body: JSONObject? = null): JSONObject {
        val connection = open(method, url)
        if (body != null) {
            connection.doOutput = true
            connection.setRequestProperty("Content-Type", "application/json; charset=UTF-8")
            connection.outputStream.use { it.write(body.toString().toByteArray()) }
        }
        return read(connection)
    }

    private fun open(method: String, url: String): HttpURLConnection {
        val connection = URL(url).openConnection() as HttpURLConnection
        // HttpURLConnection has no PATCH: Drive accepts this override.
        if (method == "PATCH") {
            connection.requestMethod = "POST"
            connection.setRequestProperty("X-HTTP-Method-Override", "PATCH")
        } else {
            connection.requestMethod = method
        }
        connection.connectTimeout = TIMEOUT_MS
        connection.readTimeout = TIMEOUT_MS
        connection.setRequestProperty("Authorization", "Bearer $token")
        return connection
    }

    private fun read(connection: HttpURLConnection): JSONObject {
        check(connection)
        return JSONObject(connection.inputStream.use(InputStream::readUtf8))
    }

    /** Throws [DriveException] for an HTTP error (its body, without secrets, is only logged). */
    private fun check(connection: HttpURLConnection) {
        val code = connection.responseCode
        if (code in 200..299) return
        val detail = connection.errorStream?.use(InputStream::readUtf8).orEmpty()
        android.util.Log.i("Passel", "Drive: HTTP $code ${detail.take(300)}")
        connection.disconnect()
        throw DriveException(code, "HTTP $code")
    }

    private fun file(json: JSONObject) = DriveFile(
        id = json.getString("id"),
        name = json.optString("name"),
        version = json.optString("version"),
        modifiedTime = json.optString("modifiedTime").ifEmpty { null },
    )

    companion object {
        const val SUFFIX = ".tar.xz"
        private const val API = "https://www.googleapis.com/drive/v3"
        private const val UPLOAD = "https://www.googleapis.com/upload/drive/v3"
        private const val FIELDS = "id,name,version,modifiedTime"
        private const val MIME_TYPE = "application/x-xz"
        private const val TAG_KEY = "passel"
        private const val TAG_VAULT = "vault"
        private const val TAG_FOLDER = "folder"
        private const val TIMEOUT_MS = 30_000

        private fun encode(value: String) = URLEncoder.encode(value, "UTF-8").replace("+", "%20")
    }
}

private fun InputStream.readUtf8(): String = readBytes().toString(Charsets.UTF_8)

private fun List<String>.toJsonArray() = org.json.JSONArray(this)
