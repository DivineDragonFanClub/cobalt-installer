package dev.dioxus.main

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.util.Log
import java.io.InputStream
import java.io.OutputStream
import java.util.zip.ZipInputStream

private const val TAG = "CobaltInstall"

// Eden's own document provider. We only ever install into Eden's storage, so we lock
// the folder grant to this provider's root (Eden's data dir) instead of letting the
// user roam anywhere on the device.
private const val EDEN_AUTHORITY = "dev.eden.eden_emulator.user"
// Document id used only to point the picker at Eden's root. The grant itself comes back
// as "root/" (with the trailing slash), but the picker's initial-location hint resolves
// with the plain "root" form, like the "root/sdmc" hint did.
private const val EDEN_ROOT_DOC_ID = "root"

/*
 * Workaround for a dx bug: dx's own generated Logger.kt (package dev.dioxus.main)
 * references BuildConfig unqualified, but Gradle generates BuildConfig in the app's
 * namespace package (our [bundle] identifier, com.divinedragonfanclub), not here. So
 * the reference doesn't resolve and Kotlin compilation fails. We provide a BuildConfig
 * in this package to satisfy it. Different package from Gradle's, so no collision.
 * If dx is ever fixed to put the glue package and namespace in sync, delete this.
 */
object BuildConfig {
    const val DEBUG = true
}

/*
 * Android side of the Cobalt installer.
 *
 * The Rust UI (src/main.rs, the `saf` module) calls the four public methods here over
 * JNI. Everything that touches Eden's folder happens here, through the Storage Access
 * Framework (content URIs), because Eden's data lives under Android/data.
 *
 * Two things that shaped this code:
 *  - Eden is a yuzu fork, so it loads exefs mods from load/<TitleID>/<ModName>/exefs/,
 *    NOT the atmosphere/contents/ layout inside the Cobalt zip. We remap on the fly.
 *    load/ and sdmc/ are siblings under Eden's data dir, so the grant must be Eden's
 *    root, not sdmc.
 *  - Eden's own DocumentsProvider is limited: it can't list a directory's children
 *    (that throws), and createDocument fails over an existing name. So we build child
 *    document ids by path and, to update a file, just open the existing document for
 *    writing. We only create (and walk/make parent dirs) when the open shows it's new.
 *
 * WryActivity (dx/wry base) is an AppCompatActivity, so startActivityForResult /
 * onActivityResult are available.
 */
class MainActivity : WryActivity() {

    private val PREFS = "cobalt"
    private val KEY_TREE = "tree_uri"
    // Outcome of the most recent folder pick: 0 = none/pending, 1 = granted, 2 = wrong folder.
    private val KEY_OUTCOME = "pick_outcome"
    private val OUTCOME_GRANTED = 1
    private val OUTCOME_REJECTED = 2
    private val REQ_OPEN_TREE = 4242

    private fun prefs() = getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    // --- called from Rust over JNI ---

    // saf::request_tree_access -> "()V"
    fun requestTreeAccess() {
        // Reset the outcome so we only report the result of this fresh pick.
        prefs().edit().remove(KEY_OUTCOME).apply()
        runOnUiThread {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
                addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                )
                // Open the picker straight at Eden's root so the user just taps "use this
                // folder". Anything else is rejected in onActivityResult.
                val edenRoot = DocumentsContract.buildDocumentUri(EDEN_AUTHORITY, EDEN_ROOT_DOC_ID)
                putExtra(DocumentsContract.EXTRA_INITIAL_URI, edenRoot)
            }
            startActivityForResult(intent, REQ_OPEN_TREE)
        }
    }

    // saf::persisted_tree_uri -> "()Ljava/lang/String;", returns null if not granted
    fun getPersistedTreeUri(): String? {
        val saved = prefs().getString(KEY_TREE, null) ?: return null
        if (!isEdenRoot(Uri.parse(saved))) return null
        val stillGranted = contentResolver.persistedUriPermissions.any {
            it.uri.toString() == saved && it.isWritePermission
        }
        return if (stillGranted) saved else null
    }

    // saf::pick_outcome -> "()I", result of the most recent pick (see OUTCOME_* above)
    fun pickOutcome(): Int = prefs().getInt(KEY_OUTCOME, 0)

    // saf::install_zip -> "([B)Z", true only if every file was written
    fun installZip(bytes: ByteArray): Boolean {
        val treeUri = getPersistedTreeUri()?.let { Uri.parse(it) } ?: return false
        val rootId = DocumentsContract.getTreeDocumentId(treeUri).trimEnd('/')
        Log.i(TAG, "installZip: rootId='$rootId'")
        var allOk = true
        return try {
            ZipInputStream(bytes.inputStream()).use { zip ->
                var entry = zip.nextEntry
                while (entry != null) {
                    if (!entry.isDirectory) {
                        val rel = remap(entry.name)
                        Log.i(TAG, "entry='${entry.name}' -> '$rel'")
                        if (!writeFile(treeUri, rootId, rel, zip)) allOk = false
                    }
                    zip.closeEntry()
                    entry = zip.nextEntry
                }
            }
            Log.i(TAG, "installZip: done allOk=$allOk")
            allOk
        } catch (e: Exception) {
            Log.e(TAG, "installZip failed", e)
            false
        }
    }

    // saf::delete_bad_subsdk9 -> "()Z"
    // The desktop build cleans up a stray subsdk9 from an old atmosphere-style layout.
    // On Eden Android the loader lives in load/<TID>/Cobalt/exefs/ and we overwrite it
    // directly, so there's nothing to pre-clean. No-op on purpose.
    fun deleteBadSubsdk9(): Boolean = false

    // --- picker result ---

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQ_OPEN_TREE && resultCode == RESULT_OK) {
            val uri = data?.data ?: return
            // Only accept Eden's own root folder, so the install target stays fixed. The
            // most recent pick always wins: a wrong folder clears any previous grant so
            // the UI can't keep showing "granted" off a stale one.
            if (!isEdenRoot(uri)) {
                Log.w(TAG, "rejected grant, not Eden's root: $uri")
                prefs().edit().remove(KEY_TREE).putInt(KEY_OUTCOME, OUTCOME_REJECTED).apply()
                return
            }
            contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            )
            prefs().edit().putString(KEY_TREE, uri.toString()).putInt(KEY_OUTCOME, OUTCOME_GRANTED).apply()
        }
    }

    // --- helpers (path/doc-id based, we never list children) ---

    // Map a zip entry to where it belongs in Eden's storage.
    private fun remap(name: String): String {
        val clean = name.trimStart('/')
        val parts = clean.split('/')
        // atmosphere/contents/<TID>/exefs/... is the LayeredFS exefs patch. Eden loads
        // those from load/<TID>/<modname>/exefs/, so send it there under a "Cobalt" mod.
        if (parts.size >= 4 && parts[0] == "atmosphere" && parts[1] == "contents") {
            val tid = parts[2]
            val rest = parts.drop(3).joinToString("/")
            return "load/$tid/Cobalt/$rest"
        }
        // Everything else (the engage/ data) goes on the emulated SD card.
        return "sdmc/$clean"
    }

    // True only for Eden's own provider root, our one allowed install target.
    private fun isEdenRoot(treeUri: Uri): Boolean {
        if (treeUri.authority != EDEN_AUTHORITY) return false
        val id = try {
            DocumentsContract.getTreeDocumentId(treeUri)
        } catch (e: Exception) {
            return false
        }
        val leaf = id.trimEnd('/').substringAfterLast('/').substringAfterLast(':')
        return leaf.equals("root", ignoreCase = true)
    }

    // Write one file. Fast path: open the existing document and overwrite it (this is
    // the update case, and it sidesteps every one of Eden's provider quirks). Slow path,
    // only when the file is new: make the parent dirs, create the document, then write.
    private fun writeFile(treeUri: Uri, rootId: String, relPath: String, input: InputStream): Boolean {
        val fileUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, "$rootId/$relPath")
        var out = openWrite(fileUri)
        if (out == null) {
            val parentPath = relPath.substringBeforeLast('/', "")
            val name = relPath.substringAfterLast('/')
            val dirId = ensureDir(treeUri, rootId, parentPath)
            if (dirId == null) {
                Log.e(TAG, "could not open/create parent dirs for '$relPath'")
                return false
            }
            val parentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, dirId)
            val created = try {
                DocumentsContract.createDocument(contentResolver, parentUri, "application/octet-stream", name)
            } catch (e: Exception) {
                Log.e(TAG, "createDocument('$name') under '$dirId' threw", e)
                null
            } ?: run {
                Log.e(TAG, "could not create '$relPath'")
                return false
            }
            out = openWrite(created)
        }
        if (out == null) {
            Log.e(TAG, "no output stream for '$relPath'")
            return false
        }
        out.use { input.copyTo(it) }
        Log.i(TAG, "wrote '$relPath'")
        return true
    }

    // Open a document for overwriting. "wt" truncates, some providers only accept "w".
    private fun openWrite(uri: Uri): OutputStream? {
        for (mode in arrayOf("wt", "w")) {
            try {
                contentResolver.openOutputStream(uri, mode)?.let { return it }
            } catch (e: Exception) {
                // not writable in this mode / not there, try next or fall back to create
            }
        }
        return null
    }

    // Walk the path, creating folders that don't exist. Returns the leaf folder's doc id.
    private fun ensureDir(treeUri: Uri, rootId: String, path: String): String? {
        var docId = rootId
        for (part in path.split('/')) {
            if (part.isEmpty()) continue
            val childId = "$docId/$part"
            if (docExists(DocumentsContract.buildDocumentUriUsingTree(treeUri, childId))) {
                docId = childId
                continue
            }
            val parentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, docId)
            val created = try {
                DocumentsContract.createDocument(
                    contentResolver, parentUri, DocumentsContract.Document.MIME_TYPE_DIR, part
                )
            } catch (e: Exception) {
                Log.e(TAG, "createDocument(dir '$part') under '$docId' threw", e)
                null
            } ?: return null
            docId = DocumentsContract.getDocumentId(created)
            Log.i(TAG, "created dir '$part' -> '$docId'")
        }
        return docId
    }

    private fun docExists(docUri: Uri): Boolean {
        return try {
            contentResolver.query(
                docUri, arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID), null, null, null
            )?.use { it.moveToFirst() } ?: false
        } catch (e: Exception) {
            false
        }
    }
}
