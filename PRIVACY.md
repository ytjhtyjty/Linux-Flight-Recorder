# Privacy

Local, no telemetry, no collection-time network access. Runtime commands are
allowlisted system tools queried read-only; they do not update packages.
Collection is structurally minimal. No serial fields, EDID, SSID, MAC/IP, routes,
process environment, ExecStart, command line, audio or screen are requested.
No /etc/shadow, SSH keys, browser profiles, shell history or documents are read.
Paths mentioned in logs are never opened.

Journalctl requests only MESSAGE, unit, transport and priority; compulsory
cursor/timestamp/boot metadata is reduced to known fields. Unknown fields are
dropped. Non-scalar fields are omitted visibly. Logs can contain arbitrary personal
data. Known local hostname, USER and HOME strings are replaced in free-text fields
and labels. Additional patterns redact IP/MAC, home paths, emails and obvious
credential assignments. This is deliberately not a claim of full anonymity.
Hostnames/users not available through the known context may remain in free text.

Local storage keeps boot ID and journal cursor for evidence; public exports remove
them. Public exports contain exactly snapshot.json, manifest.json and PRIVACY.txt.
The manifest hashes exported content; it is not a digital signature. Device IDs,
package versions and topological locations remain useful and potentially identifying.
There are no pseudonyms, global machine IDs or exported secrets used for matching.
`export --preview` lists included files and transformation classes, not a promise
that every sensitive substring was found. Extract into a new private directory and
review all three files before public posting. Deleting local files is not secure
erasure on SSDs, snapshots or backups.

Reports escape control/bidi/Markdown-active characters. Exported JSON escapes JSON
controls normally and MUST NOT be rendered as trusted Markdown/HTML or printed raw
with a tool that interprets terminal escapes. The CLI JSON serializer escapes ASCII
controls; bidi and printable Unicode are data, not instructions.
