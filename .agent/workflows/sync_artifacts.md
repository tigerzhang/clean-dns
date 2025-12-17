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
   cp /Users/zhanghu/.gemini/antigravity/brain/bef3ae87-c1bd-44d9-9dd7-8f65bf2b27f1/{implementation_plan.md,task.md,walkthrough.md} docs/
   ```

3. Copy versioned backups.
   // turbo
   ```bash
   cp /Users/zhanghu/.gemini/antigravity/brain/bef3ae87-c1bd-44d9-9dd7-8f65bf2b27f1//{implementation_plan.md,task.md,walkthrough.md}.* docs/ || true
   ```
