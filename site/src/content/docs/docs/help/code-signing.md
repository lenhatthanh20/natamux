---
title: Code signing policy
description: How Natamux release files are built and signed, who may change the code, and who approves a signature.
sidebar:
  order: 4
---

:::note[Not active yet]
Natamux has applied for free code signing from SignPath Foundation. Until the project is accepted, the Windows files are not code-signed. This page says how signing will work.
:::

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

## What is signed

| File | Platform |
|---|---|
| `Natamux-win-x64.exe` (the installer) | Windows |
| `natamux-app.exe`, `natamux-daemon.exe`, `natamux.exe`, `nmx.exe` (inside the installer) | Windows |

Only files that the [Release workflow](https://github.com/lenhatthanh20/natamux/blob/main/.github/workflows/release.yml) builds on GitHub Actions from this repository are signed. Every file has the product name **Natamux** and the release version in its file details.

The macOS files are not part of this policy. Every release, on every platform, also has `SHA256SUMS.txt` and its minisign signature: see [Verify the download](/docs/getting-started/install/#verify-the-download).

## Team roles

| Role | Who |
|---|---|
| Committers and reviewers | [lenhatthanh20](https://github.com/lenhatthanh20) |
| Approvers | [lenhatthanh20](https://github.com/lenhatthanh20) |

- Committers may change the source code without another review.
- A reviewer reviews every pull request from someone who is not a committer before it is merged. Build scripts and CI files count as source code.
- An approver approves every signing request by hand, one release at a time.
- Every team member uses multi-factor authentication for GitHub and SignPath.

## Privacy policy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

The full list of connections, and the privacy policies of the services involved, is in [PRIVACY.md](https://github.com/lenhatthanh20/natamux/blob/main/PRIVACY.md).

## Report a problem

To report a signed file that looks wrong, follow [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md).
