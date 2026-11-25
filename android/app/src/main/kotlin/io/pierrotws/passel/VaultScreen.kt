package io.pierrotws.passel

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VaultScreen(state: VaultState, model: VaultViewModel) {
    var menu by remember { mutableStateOf(false) }
    var confirmRemove by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Passel") },
                actions = {
                    IconButton(onClick = { model.sync() }, enabled = state.busy == null) {
                        BadgedBox(badge = { if (state.unsynced > 0) Badge { Text("${state.unsynced}") } }) {
                            Icon(Icons.Default.Refresh, contentDescription = "Sync")
                        }
                    }
                    IconButton(onClick = model::lock) { Icon(Icons.Default.Lock, contentDescription = "Lock") }
                    IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, contentDescription = "More") }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(
                            text = { Text("Remove the vault from this phone") },
                            onClick = { menu = false; confirmRemove = true },
                        )
                    }
                },
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = model::startCreating) {
                Icon(Icons.Default.Add, contentDescription = "New entry")
            }
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            if (state.busy != null) LinearProgressIndicator(Modifier.fillMaxWidth())
            OutlinedTextField(
                value = state.query,
                onValueChange = model::setQuery,
                placeholder = { Text("Search ${state.entries.size} entries") },
                leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            )
            val groups = state.filtered.groupBy { it.folder }
            LazyColumn(Modifier.fillMaxSize()) {
                groups.forEach { (folder, entries) ->
                    if (folder.isNotEmpty()) {
                        item(key = "folder:$folder") {
                            Text(
                                folder,
                                style = MaterialTheme.typography.labelLarge,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp),
                            )
                        }
                    }
                    items(entries, key = { it.path }) { entry ->
                        ListItem(
                            modifier = Modifier.clickable(enabled = state.busy == null) { model.open(entry) },
                        ) { Text(entry.name) }
                        HorizontalDivider()
                    }
                }
            }
        }
    }

    if (confirmRemove) {
        AlertDialog(
            onDismissRequest = { confirmRemove = false },
            title = { Text("Remove the vault from this phone?") },
            text = {
                val unsynced = if (state.unsynced > 0) " ${state.unsynced} change(s) not synced yet will be lost." else ""
                Text("The copy on your remote stays as it is; you can download it again.$unsynced")
            },
            confirmButton = {
                TextButton(onClick = { confirmRemove = false; model.removeVault() }) { Text("Remove") }
            },
            dismissButton = { TextButton(onClick = { confirmRemove = false }) { Text("Cancel") } },
        )
    }
}
