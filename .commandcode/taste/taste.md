# Taste (Continuously Learned by [CommandCode][cmd])

[cmd]: https://commandcode.ai/

# workflow
- Commit and push changes to the dev branch after completing code changes. Confidence: 0.85
- Exclude "Co-authored-by: CommandCodeBot" and similar attribution from commit messages. Confidence: 0.82

# architecture
See [architecture/taste.md](architecture/taste.md)
# workflow
- Build from the ground up and verify each step against the cua repo before proceeding — incremental, step-by-step validation at every stage. Confidence: 0.80
- Commit and push changes to the dev branch after completing code changes. Confidence: 0.85
- Exclude "Co-authored-by: CommandCodeBot" and similar attribution from commit messages. Confidence: 0.82
- Before claiming a tag or commit is already pushed/synced, run `git fetch` and re-check `git log` to verify current remote state — do not rely on a previous `git log` from earlier in the conversation. Confidence: 0.70
