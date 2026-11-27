package io.pierrotws.passel

import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.security.MessageDigest

/**
 * Copies of a vault in a place the user picked with the system's file
 * picker (this phone, an SD card, a USB drive, a storage app such as
 * Nextcloud or Syncthing). Android gives no file path there, only
 * documents: Passel reads and writes them whole, and libgit2 works on
 * scratch copies in the app's storage.
 */
object LocalCopy {
    /** Name of the folder copy made in the folder the user picks. */
    const val FOLDER_NAME = "passwords.git"

    /** Suggested name of a file copy. */
    const val FILE_NAME = "passwords.bundle"

    /** Keeps access to [uri] across restarts. */
    fun keepAccess(context: Context, uri: Uri) {
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        runCatching { context.contentResolver.takePersistableUriPermission(uri, flags) }
    }

    /** What to show for a picked document or folder. */
    fun displayName(context: Context, uri: Uri): String? {
        val document = if (DocumentsContract.isTreeUri(uri) && !DocumentsContract.isDocumentUri(context, uri)) {
            DocumentsContract.buildDocumentUriUsingTree(uri, DocumentsContract.getTreeDocumentId(uri))
        } else {
            uri
        }
        return runCatching {
            context.contentResolver.query(document, arrayOf(Document.COLUMN_DISPLAY_NAME), null, null, null)
                ?.use { if (it.moveToFirst()) it.getString(0) else null }
        }.getOrNull()
    }
}

/** The folder copy changed while this device was writing it: sync again. */
class CopyChangedException : IOException("the copy changed meanwhile")

/** The picked folder already holds a vault: download it instead. */
class CopyExistsException : IOException("the folder already holds a vault")

/** A vault's copy in one file (a git bundle). */
class FileCopy(context: Context, val uri: Uri) {
    private val resolver = context.contentResolver

    /** Copies the file to [to]; returns its digest, or null when it is empty (never written). */
    fun read(to: File): String? {
        val input = resolver.openInputStream(uri) ?: throw FileNotFoundException(uri.toString())
        input.use { stream -> to.outputStream().use { stream.copyTo(it) } }
        return if (to.length() == 0L) null else digest(to)
    }

    /** The file's digest now (null when empty), to see whether it changed. */
    fun currentDigest(scratch: File): String? = read(scratch)

    fun write(from: File) {
        val output = resolver.openOutputStream(uri, "wt") ?: throw FileNotFoundException(uri.toString())
        output.use { stream -> from.inputStream().use { it.copyTo(stream) } }
    }
}

/**
 * A vault's copy in a folder: a bare git repository, the same as the
 * desktop app's folder copy, so a computer can clone or sync it as it
 * is. [uri] is the repository folder's document (inside the tree the
 * user gave access to).
 */
class FolderCopy(context: Context, val uri: Uri) {
    private val resolver: ContentResolver = context.contentResolver
    private val rootId: String = DocumentsContract.getDocumentId(uri)

    /** What the folder held when read: each file's document and digest. */
    class Snapshot internal constructor(
        internal val files: Map<String, Pair<Uri, String>>,
        internal val dirs: MutableMap<String, Uri>,
    ) {
        /** No repository there yet (a copy never written). */
        val isEmpty: Boolean get() = "HEAD" !in files
    }

    /** Copies the repository into [into] (an empty folder); hooks are left out. */
    fun download(into: File): Snapshot {
        val files = mutableMapOf<String, Pair<Uri, String>>()
        val dirs = mutableMapOf("" to uri)
        fun walk(docId: String, path: String, dir: File) {
            for (child in children(resolver, uri, docId)) {
                val childPath = if (path.isEmpty()) child.name else "$path/${child.name}"
                if (childPath == "hooks" || !safeName(child.name)) continue
                val childUri = DocumentsContract.buildDocumentUriUsingTree(uri, child.id)
                val local = File(dir, child.name)
                if (child.isDir) {
                    local.mkdirs()
                    dirs[childPath] = childUri
                    walk(child.id, childPath, local)
                } else {
                    val input = resolver.openInputStream(childUri) ?: throw FileNotFoundException(childPath)
                    input.use { stream -> local.outputStream().use { stream.copyTo(it) } }
                    files[childPath] = childUri to digest(local)
                }
            }
        }
        into.mkdirs()
        walk(rootId, "", into)
        return Snapshot(files, dirs)
    }

    /**
     * Writes what changed in [from] since [snapshot]: the objects first,
     * the refs last, after checking that nobody moved them meanwhile
     * ([CopyChangedException]).
     */
    fun upload(from: File, snapshot: Snapshot) {
        val changed = from.walkTopDown()
            .filter { it.isFile }
            .map { it.relativeTo(from).invariantSeparatorsPath to it }
            .filter { (path, _) -> !path.startsWith("hooks/") }
            .filter { (path, file) -> snapshot.files[path]?.second != digest(file) }
            .toList()
        val (refs, data) = changed.partition { (path, _) -> isRef(path) }
        data.forEach { (path, file) -> write(path, file, snapshot) }
        // The refs as read must still be there: else another device wrote
        // in between, and its commits would be lost.
        for ((_, entry) in snapshot.files.filterKeys(::isRef)) {
            val now = resolver.openInputStream(entry.first)?.use { digest(it.readBytes()) }
            if (now != entry.second) throw CopyChangedException()
        }
        refs.forEach { (path, file) -> write(path, file, snapshot) }
    }

    private fun write(path: String, file: File, snapshot: Snapshot) {
        val document = snapshot.files[path]?.first ?: run {
            val parent = ensureDir(path.substringBeforeLast('/', ""), snapshot)
            DocumentsContract.createDocument(resolver, parent, MIME_BINARY, path.substringAfterLast('/'))
                ?: throw IOException("could not create $path")
        }
        val output = resolver.openOutputStream(document, "wt") ?: throw FileNotFoundException(path)
        output.use { stream -> file.inputStream().use { it.copyTo(stream) } }
    }

    private fun ensureDir(path: String, snapshot: Snapshot): Uri {
        snapshot.dirs[path]?.let { return it }
        val parent = ensureDir(path.substringBeforeLast('/', ""), snapshot)
        val made = DocumentsContract.createDocument(resolver, parent, Document.MIME_TYPE_DIR, path.substringAfterLast('/'))
            ?: throw IOException("could not create $path")
        snapshot.dirs[path] = made
        return made
    }

    companion object {
        private const val MIME_BINARY = "application/octet-stream"

        /**
         * The copy to make in the folder [tree] the user picked: its
         * [LocalCopy.FOLDER_NAME] folder, made when missing. One holding a
         * vault already is refused ([CopyExistsException]).
         */
        fun create(context: Context, tree: Uri): FolderCopy {
            val resolver = context.contentResolver
            val treeId = DocumentsContract.getTreeDocumentId(tree)
            val existing = children(context, tree, treeId).firstOrNull { it.name == LocalCopy.FOLDER_NAME }
            val folder = when {
                existing == null -> DocumentsContract.createDocument(
                    resolver,
                    DocumentsContract.buildDocumentUriUsingTree(tree, treeId),
                    Document.MIME_TYPE_DIR,
                    LocalCopy.FOLDER_NAME,
                ) ?: throw IOException("could not create the folder")
                children(context, tree, existing.id).isNotEmpty() -> throw CopyExistsException()
                else -> DocumentsContract.buildDocumentUriUsingTree(tree, existing.id)
            }
            return FolderCopy(context, folder)
        }

        /**
         * The copy in the folder [tree] the user picked: the folder itself
         * when it is a repository, else a repository in it
         * ([LocalCopy.FOLDER_NAME] first); null when there is none.
         */
        fun find(context: Context, tree: Uri): FolderCopy? {
            val treeId = DocumentsContract.getTreeDocumentId(tree)
            val inside = children(context, tree, treeId)
            if (isRepository(inside)) return FolderCopy(context, DocumentsContract.buildDocumentUriUsingTree(tree, treeId))
            return inside
                .filter { it.isDir && it.name.endsWith(".git") }
                .sortedByDescending { it.name == LocalCopy.FOLDER_NAME }
                .firstOrNull { isRepository(children(context, tree, it.id)) }
                ?.let { FolderCopy(context, DocumentsContract.buildDocumentUriUsingTree(tree, it.id)) }
        }

        private fun isRepository(children: List<Child>) =
            children.any { it.name == "HEAD" && !it.isDir } && children.any { it.name == "objects" && it.isDir }

        private fun children(context: Context, tree: Uri, docId: String) = children(context.contentResolver, tree, docId)

        private fun children(resolver: ContentResolver, tree: Uri, docId: String): List<Child> {
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, docId)
            val columns = arrayOf(Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE)
            val cursor = resolver.query(children, columns, null, null, null)
                ?: throw FileNotFoundException(tree.toString())
            return cursor.use {
                buildList {
                    while (it.moveToNext()) add(Child(it.getString(0), it.getString(1), it.getString(2) == Document.MIME_TYPE_DIR))
                }
            }
        }

        private fun isRef(path: String) = path == "HEAD" || path == "packed-refs" || path.startsWith("refs/")

        /** A plain name: nothing a crafted provider could use to escape the scratch folder. */
        private fun safeName(name: String) = name.isNotEmpty() && name != "." && name != ".." && '/' !in name && '\\' !in name
    }

    private data class Child(val id: String, val name: String, val isDir: Boolean)
}

private fun digest(file: File): String = file.inputStream().use { input ->
    val sha = MessageDigest.getInstance("SHA-256")
    val buffer = ByteArray(64 * 1024)
    while (true) {
        val read = input.read(buffer)
        if (read < 0) break
        sha.update(buffer, 0, read)
    }
    sha.digest().joinToString("") { "%02x".format(it) }
}

private fun digest(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
