## What changes for the player?
<!-- Describe what players will notice. One or two sentences are enough. -->

## How to test
<!-- What testers should do with the test build (linked in a comment below once it is built): the map, the bus, the steps, and what should happen. -->

## Screenshots
<!-- Required when something changes on screen, in the launcher or in the game: before and after, same place and same settings. Drag the images here; do not commit them. Write "No visible change" otherwise. -->

## What does this change, and why?
<!-- Briefly explain the approach and the reason for it. Point out anything reviewers should look at first. -->

## What could break?
<!-- What else uses the code you changed, and what you could not test yourself (a platform, VR, online play, a map, a mod). "Nothing that I know of" is a fine answer for a small fix. -->

## Checklist
<!-- Tick what applies; strike through (~~like this~~) what does not apply to this change. -->
- [ ] `cargo test --workspace` and `cargo build --release` pass
- [ ] The pull request contains a single change
- [ ] Before/after screenshots are in the description if anything changes on screen
- [ ] New or changed interface texts are in `crates/omsi-app/locales/app.yml`, in every language I can
- [ ] Online play, saves, settings and mods made for the previous release still work, or the description says what changes
- [ ] No original OMSI 2 content and no file of a megabyte or more in the commits
