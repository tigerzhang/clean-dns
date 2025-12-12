---
description: "Syncs the implementation plan, task list, and walkthrough artifacts to the docs directory."
---

1. Create the docs directory if it doesn't exist.

   ```bash
   mkdir -p docs
   ```

2. Copy the artifacts from the brain directory.
   // turbo

   ```bash
   cp /Users/zhanghu/.gemini/antigravity/brain/625eb765-0296-4916-aa79-17774c92a05d/{implementation_plan.md,task.md,walkthrough.md} docs/
   ```

3. (Optional) Copy versioned backups.
   // turbo
   ```bash
   cp /Users/zhanghu/.gemini/antigravity/brain/625eb765-0296-4916-aa79-17774c92a05d/{implementation_plan.md,task.md,walkthrough.md}.* docs/ || true
   ```
