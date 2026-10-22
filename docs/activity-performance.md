# Activity and graph speed

Repository contribution calendars live on the Activity tab. The initial page defers the
calendar scan to a fragment request; a normal link also loads it without JavaScript.
The Commits tab shows branch history and pagination without loading a calendar.

Repository calendars cache daily totals by the current default-branch commit. Warm year views read only the requested days. The cache holds at most 32 tips and 25,000 day entries. Pushes, force pushes and default-branch changes select a new key. Partial history still shows the partial notice. Requests for partial days keep their exact second bounds.

Commit graphs use a priority queue to order children before parents while keeping the same date priority. Each row builds a lookup for lane destinations, so edge lookup does not scan all lanes for each edge. Merge parents, track colors, labels and the 200-commit limit stay the same. Calendar tooltips reuse one date formatter.
