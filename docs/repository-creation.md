# Create a repository

Use the Empty, Template, Mirror and Import tabs on the new repository page. Tabs load each form once, then keep it in the page. Owner, name, description and visibility carry across the forms that support them. Template sources, mirror URLs, fetch intervals and import options stay with their own tab.

Switching tabs keeps drafts in the current page only. Submitting a form uses the same server route and access checks as before. If a tab cannot load, it shows a retry message and keeps your current form. Without JavaScript, the tabs open each form as a page.

## Start from a template

Enter a repository as `owner/name` on the Template tab, or open the source repository's Clone menu and choose **Use as template**. rbit copies the files from the source's default branch into one initial commit on the new repository's configured default branch. The new repository has its own history and settings.

Anyone signed in can use a public repository as a template. Using a private source requires administration access to that repository and a private destination. If Git LFS is enabled, rbit links the objects referenced by the snapshot that are already linked to the source or available from a public repository in its fork network.
