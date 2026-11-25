package io.pierrotws.passel

import android.content.res.Configuration
import java.util.concurrent.ConcurrentHashMap
import io.pierrotws.passel.core.setLanguages as rustSetLanguages
import io.pierrotws.passel.core.translate
import io.pierrotws.passel.core.translateContext
import io.pierrotws.passel.core.translatePlural

/**
 * Translations, from the same gettext catalogs as the desktop app
 * (`po/<lang>.po` at the repository root, compiled into the Rust library).
 *
 * Messages are written in English and looked up with [tr], [trn] and
 * [trp]; `po/update.sh` extracts them from these calls, so the first
 * arguments must be plain string literals (no `$` templates). Values go
 * in named placeholders, filled after translation:
 *
 * ```
 * tr("Deleted {name}", "name" to entry.name)
 * trn("{n} entry", "{n} entries", count, "n" to count)
 * ```
 */
object I18n {
    private val cache = ConcurrentHashMap<String, String>()

    /** The language in use, e.g. `fr` or `en`. */
    @Volatile
    var language: String = "en"
        private set

    /**
     * Follows [configuration]'s locales (the system's, or the per-app
     * language on Android 13+). Call before showing anything: from
     * `Application.onCreate` and each `Activity.onCreate`, which runs
     * again when the language changes.
     */
    fun apply(configuration: Configuration) {
        val tags = configuration.locales.toLanguageTags().split(',').filter(String::isNotBlank)
        val chosen = rustSetLanguages(tags)
        if (chosen != language) cache.clear()
        language = chosen
    }

    internal fun lookup(key: String, load: () -> String): String = cache.getOrPut(key, load)
}

/** The translation of [msgid], placeholders filled from [args]. */
fun tr(msgid: String, vararg args: Pair<String, Any?>): String =
    formatNamed(I18n.lookup(msgid) { translate(msgid) }, args)

/** The translation of [msgid] in [context], for an English text with several meanings. */
fun trp(context: String, msgid: String, vararg args: Pair<String, Any?>): String =
    formatNamed(I18n.lookup("$context\u0004$msgid") { translateContext(context, msgid) }, args)

/** The singular or plural translation for [n], by the language's plural rules. */
fun trn(singular: String, plural: String, n: Number, vararg args: Pair<String, Any?>): String {
    val count = n.toLong().coerceAtLeast(0)
    return formatNamed(translatePlural(singular, plural, count.toULong()), args)
}

/**
 * Replaces each `{name}` with its value from [args], in one pass, like
 * passel-core's `i18n::format_named`. Unknown placeholders stay as they are.
 */
internal fun formatNamed(template: String, args: Array<out Pair<String, Any?>>): String {
    if (args.isEmpty()) return template
    val out = StringBuilder(template.length)
    var i = 0
    while (i < template.length) {
        val open = template.indexOf('{', i)
        if (open < 0) break
        out.append(template, i, open)
        val close = template.indexOf('}', open + 1)
        val value = if (close > 0) args.firstOrNull { it.first == template.substring(open + 1, close) } else null
        if (value != null) {
            out.append(value.second.toString())
            i = close + 1
        } else {
            out.append('{')
            i = open + 1
        }
    }
    out.append(template, i, template.length)
    return out.toString()
}
