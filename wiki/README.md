# The wiki

The pages that make up the GitHub wiki of tql, kept in the repository so they
can be reviewed in the same pull requests as the code. The README and `docs/`
are the source of truth; these pages adapt them into cross-linked wiki pages and
point back to the docs where the detail lives.

| Page | |
| --- | --- |
| `Home.md` | the landing page the wiki opens on |
| `Installation.md`, `Connecting.md` | getting in |
| `Browsing-Data.md`, `Editing-Data.md`, `Row-Inspector.md` | using the browser |
| `SQL-Editor.md`, `Asking-for-SQL.md` | writing SQL |
| `Exporting.md`, `Dump-Load-and-Sync.md` | moving data |
| `Commands.md`, `Keys.md`, `Configuration.md` | reference |
| `For-Agents.md`, `Troubleshooting.md` | the rest of the answers |
| `Developing.md`, `Packaging-and-Releasing.md`, `Changelog.md` | the project |
| `_Sidebar.md`, `_Footer.md` | the wiki's navigation |

## Publishing

The GitHub wiki is a separate git repository, `https://github.com/VheissuLabs/tql.wiki.git`,
and GitHub does not create it until the first page has been made on the web
site. Once it exists, publish an update from a checkout of this folder:

```bash
git clone https://github.com/VheissuLabs/tql.wiki.git
cd tql.wiki
cp -R <this folder>/. .
git add -A && git commit -m 'wiki: update pages' && git push origin master
```

Publishing copies `README.md` too if it lives in this folder, which is how it
becomes a wiki page; leave it out if you would rather it not be one.