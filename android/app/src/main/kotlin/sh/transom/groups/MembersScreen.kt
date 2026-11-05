package sh.transom.groups

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import sh.transom.Account
import sh.transom.Asked
import sh.transom.AskedProblem
import sh.transom.R
import sh.transom.ask
import sh.transom.theme.ActionButton
import sh.transom.theme.BodyLineHeight
import sh.transom.theme.ButtonKind
import sh.transom.theme.LineField
import sh.transom.theme.PageHeading

/**
 * The members of [group], a name without its `@`, as the web's `members.html`: the group's name,
 * each member with Remove, and Invite, which asks the invitee first. Members are peers, so any
 * member may do either. Removing yourself is leaving, so your own button says Leave, and [onLeft]
 * is called once you have left.
 */
@Composable
fun MembersScreen(
    account: Account,
    group: String,
    onLeft: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val client = account.client
    val me = "@${account.name}"
    var listings by remember { mutableIntStateOf(0) }
    var listed by remember { mutableStateOf<Asked<List<String>>?>(null) }
    LaunchedEffect(group, listings) { listed = ask { client.members(group) } }
    var asked by remember { mutableStateOf<Asked<*>?>(null) }
    var invited by remember { mutableStateOf<Invitation?>(null) }
    var typed by rememberSaveable { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    val invite = {
        val name = typed.trim().removePrefix("@")
        if (name.isNotEmpty()) {
            scope.launch {
                val answer = ask { client.invite(group, name) }
                asked = answer
                invited = (answer as? Asked.Answered)?.let { Invitation(name, it.value) }
                if (answer is Asked.Answered) typed = ""
            }
        }
    }
    Column(
        modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp, vertical = 24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        PageHeading(stringResource(R.string.who, group), Modifier.padding(top = 4.dp))
        AskedProblem(listed)
        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            (listed as? Asked.Answered)?.value?.forEach { member ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(member, lineHeight = BodyLineHeight)
                    ActionButton(
                        stringResource(
                            if (member == me) R.string.members_leave else R.string.members_remove
                        ),
                        {
                            scope.launch {
                                val removed = ask { client.remove(group, member.removePrefix("@")) }
                                asked = removed
                                invited = null
                                if (removed !is Asked.Answered) return@launch
                                if (member == me) onLeft() else listings++
                            }
                        },
                        kind = ButtonKind.Quiet,
                    )
                }
            }
        }
        val label = stringResource(R.string.members_invite)
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(label)
            LineField(
                typed,
                { typed = it },
                label,
                stringResource(R.string.members_invitee),
                onGo = invite,
                Modifier.weight(1f),
            )
            ActionButton(label, invite, kind = ButtonKind.Primary)
        }
        AskedProblem(asked)
        invited?.let {
            val said = if (it.asked) R.string.members_invited else R.string.members_already
            Text(stringResource(said, it.name), lineHeight = BodyLineHeight)
        }
        Text(
            stringResource(R.string.members_note),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            lineHeight = BodyLineHeight,
        )
    }
}

/** An invitation the server took: [name] was [asked], or was a member already. */
private data class Invitation(val name: String, val asked: Boolean)
