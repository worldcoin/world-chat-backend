# Setting Up CI Triggering for Automated PRs

## Problem
GitHub Actions won't trigger CI workflows on PRs created by other workflows using `GITHUB_TOKEN` (security feature to prevent infinite loops).

## Solution: Personal Access Token (PAT)

### Step 1: Create a Personal Access Token

1. Go to GitHub Settings → Developer settings → Personal access tokens → **Tokens (classic)**
2. Click **"Generate new token"** → **"Generate new token (classic)"**
3. Give it a name like `Proto Update Bot`
4. Set expiration (recommend 90 days and set a reminder to rotate)
5. Select scopes:
   - ✅ `repo` (Full control of private repositories)
   - That's it! Just the `repo` scope
6. Click **"Generate token"**
7. **COPY THE TOKEN NOW** (you won't see it again!)

### Step 2: Add PAT to Repository Secrets

1. Go to your repository settings: `https://github.com/worldcoin/world-chat-backend/settings/secrets/actions`
2. Click **"New repository secret"**
3. Name: `BOT_PAT`
4. Value: Paste your PAT
5. Click **"Add secret"**

### Step 3: Workflow Already Updated!

The workflow has been updated to use `BOT_PAT` if available:
```yaml
token: ${{ secrets.BOT_PAT || secrets.GITHUB_TOKEN }}
```

This means:
- If `BOT_PAT` exists → Uses PAT → CI runs on PRs ✅
- If `BOT_PAT` doesn't exist → Falls back to `GITHUB_TOKEN` → CI won't run ⚠️

## Alternative Solutions

### Option 2: GitHub App (More Complex, Better for Organizations)

1. Create a GitHub App for your organization
2. Install it on the repository
3. Use the app token in workflows

**Pros:** More secure, better audit trail, no expiration
**Cons:** More complex setup

### Option 3: Manual CI Trigger (Workaround)

If you can't use a PAT, you can manually trigger CI:
1. Close and reopen the PR
2. Push an empty commit to the PR: `git commit --allow-empty -m "Trigger CI"`
3. Use workflow_dispatch to manually run CI

## Testing After Setup

1. **With PAT configured:**
   - Run the update-proto workflow
   - If changes are detected, a PR will be created
   - CI should automatically run on the PR ✅

2. **Without PAT (current state):**
   - PR is created but CI doesn't run
   - You'll see "No checks have been run" on the PR
   - Need manual intervention to trigger CI

## Security Notes

- **Never commit the PAT** to the repository
- **Rotate PATs regularly** (every 90 days recommended)
- **Use fine-grained PATs** if you want more control (newer GitHub feature)
- **Limit PAT scope** - only give it the permissions it needs (`repo` scope)

## Quick Verification

After setting up `BOT_PAT`, test it:

```bash
# Trigger the workflow manually
# Go to Actions → Update Proto Files → Run workflow

# Or push a test change to trigger the workflow
```

The created PR should show:
- ✅ All CI checks running automatically
- ✅ No manual intervention needed
