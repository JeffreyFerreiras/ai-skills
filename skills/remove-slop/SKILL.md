---
name: remove-slop
description: Remove AI-generated slop from a branch diff. Use when the user wants to clean up unnecessary comments, abnormal defensive checks, try/catch blocks, or any style inconsistencies introduced by AI in the current branch compared to main. Reports changes in a 1-3 sentence summary.
---

# Remove Slop

Check the diff against main, and remove all AI generated slop introduced in this branch.

This includes:
- Extra comments that a human wouldn't add or is inconsistent with the rest of the file
- Extra defensive checks or try/catch blocks that are abnormal for that area of the codebase (especially if called by trusted / validated codepaths)
- Any other style that is inconsistent with the file

Report at the end with only a 1-3 sentence summary of what you changed.
