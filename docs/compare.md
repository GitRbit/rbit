# Compare changes

Choose a base and compare branch, tag or commit. Type to search each branch menu; use the arrow keys and Enter to choose, or Escape to close. You can still type a commit ID. Tag choices include `refs/tags/` to distinguish tags from branches with the same name.

Repository fields search the readable fork network. Branch menus fetch at most 50 matches from the selected repository after a short delay. A new search cancels the old one. If the search fails, type a revision and press Compare.

The diff preview uses the same file tree, line colors, syntax colors, large-file limits and offscreen blocks as commit diffs. The server builds syntax colors on a blocking worker so other requests can proceed.
