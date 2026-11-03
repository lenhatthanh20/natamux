#!/usr/bin/env python3
"""Scripted demo agent: real files, tests, hooks and stage reports; no model calls."""

import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import termios
import time
from urllib.request import Request, urlopen


ROOT = Path(os.environ.get("NATAMUX_DEMO_ROOT", "/nonexistent"))
TASK = os.environ.get("NATAMUX_TASK_ID", "")
TERMINAL = os.environ.get("NATAMUX_TERMINAL", "1")


def call(path, body):
    request = Request(os.environ["NATAMUX_URL"] + "/api" + path,
                      data=json.dumps(body).encode(), method="POST", headers={
                          "Authorization": "Bearer " + os.environ["NATAMUX_TOKEN"],
                          "Content-Type": "application/json", "X-Natamux-Task": TASK,
                          "X-Natamux-Terminal": TERMINAL})
    with urlopen(request, timeout=10) as response:
        return json.load(response)


def hook(event, **extra):
    call("/hooks/claude-code", {"hook_event_name": event, "cwd": str(Path.cwd()), **extra})


def tool(name, value, key="file_path"):
    hook("PreToolUse", tool_name=name, tool_input={key: value})
    print(f"\033[36m● {name}\033[0m  {value}", flush=True)


def pause(control, count=1):
    time.sleep(float(control.get("delay", 1.2)) * count)


def read_prompt():
    chunk = os.read(sys.stdin.fileno(), 65536)
    if not chunk:
        return None
    chunks = [chunk]
    while select.select([sys.stdin], [], [], 0.2)[0]:
        chunk = os.read(sys.stdin.fileno(), 65536)
        if not chunk:
            break
        chunks.append(chunk)
    text = b"".join(chunks).decode("utf-8")
    return text.replace("\x1b[200~", "").replace("\x1b[201~", "").strip()


def document(name, sections):
    target = Path(".natamux-task") / name
    target.parent.mkdir(exist_ok=True)
    target.write_text(sections, encoding="utf-8")
    tool("Write", str(target))
    hook("PostToolUse", tool_name="Write", tool_input={"file_path": str(target)})


def tests(expect_failure=False):
    python = os.environ["NATAMUX_DEMO_PYTHON"]
    tool("Bash", "python3 -m unittest -v", "command")
    result = subprocess.run([python, "-m", "unittest", "-v"], check=False)
    hook("PostToolUse", tool_name="Bash", tool_input={"command": "python3 -m unittest -v"})
    if expect_failure:
        if result.returncode == 0:
            raise RuntimeError("The regression should fail before the fix")
        print("The regression fails as expected; no passing result is claimed.", flush=True)
    elif result.returncode:
        raise RuntimeError("The fixture tests failed")


REGRESSIONS = '''    def test_coupon_larger_than_total(self):
        self.assertEqual(checkout(self.items, Decimal("30")), Decimal("0"))

    def test_coupon_on_empty_cart(self):
        self.assertEqual(checkout([], Decimal("5")), Decimal("0"))

    def test_coupon_equal_to_total(self):
        self.assertEqual(checkout(self.items, Decimal("24")), Decimal("0"))

'''


def add_regression():
    target = Path("test_checkout.py")
    text = target.read_text()
    if "test_coupon_larger_than_total" not in text:
        target.write_text(text.replace('if __name__ == "__main__":', REGRESSIONS + '\nif __name__ == "__main__":'))
        tool("Edit", str(target))


def negative_coupon_case():
    target = Path("test_checkout.py")
    text = target.read_text()
    if "test_negative_coupon_is_rejected" not in text:
        case = '''    def test_negative_coupon_is_rejected(self):
        with self.assertRaises(ValueError):
            checkout(self.items, Decimal("-5"))

'''
        target.write_text(text.replace('if __name__ == "__main__":', case + '\nif __name__ == "__main__":'))
        tool("Edit", str(target))


def await_continue(control):
    marker = ROOT / "controls" / (TASK + ".continue")
    while not marker.exists():
        tool("Read", "checkout.py")
        print("This build lane is paused for the recording. Waiting for Continue.", flush=True)
        pause(control, 8)


def run_stage(prompt, control):
    match = re.search(r"DEMO_STAGE=([a-z-]+)", prompt)
    if not match:
        print("Scripted demo accepts only the prepared stage prompts.", flush=True)
        return
    stage = match.group(1)
    print(f"\n\033[1m{stage.replace('-', ' ').title()}\033[0m\n", flush=True)
    hook("UserPromptSubmit")
    tool("Read", ".natamux-task/context.md")
    pause(control)
    if stage == "spec" and control["mode"] == "ask" and not (Path(".natamux-task") / ".answered").exists():
        question = "Should a negative coupon be rejected or treated as zero?"
        print("\033[33m? " + question + "\033[0m", flush=True)
        hook("Notification", notification_type="permission_prompt", message=question)
        answer = read_prompt()
        if answer is None:
            return
        print("Answer: " + answer, flush=True)
        (Path(".natamux-task") / ".answered").write_text(answer)
        hook("UserPromptSubmit")
    if stage == "spec":
        document("spec.md", "# Checkout totals\n\n## Goal\nKeep checkout totals at or above zero.\n\n## Scope\nClamp the amount after the coupon. Reject a negative coupon.\n\n## Out of scope\nTax, shipping and payment providers.\n\n## Acceptance criteria\n- An empty cart totals zero.\n- A coupon cannot make the total negative.\n- A smaller coupon reduces the amount.\n- A negative coupon raises ValueError.\n\n## Code map\n- checkout.py: checkout calculates the final amount.\n- test_checkout.py: add edge cases for coupons.\n")
    elif stage == "plan":
        document("plan.md", "# Plan\n\n## Steps\n1. Read the checkout calculation.\n2. Add coupon edge cases.\n3. Reject invalid coupons and clamp the final amount.\n\n## Files\n- checkout.py\n- test_checkout.py\n\n## Tests\nRun the checkout checks before and after the fix.\n")
    elif stage == "reproduce":
        tool("Read", "checkout.py")
        add_regression()
        tests(expect_failure=True)
        document("repro.md", "# Reproduction\n\n## Steps\nApply a 30 dollar coupon to a 24 dollar cart.\n\n## Failing test\nThe expected total is 0. The original implementation returns -6. The actual regression run failed before the fix.\n")
    elif stage == "test-spec":
        if control["mode"] == "running":
            await_continue(control)
        document("test-spec.md", "# Test spec\n\n## Cases\n- Empty cart: total 0.\n- Two 12 dollar mugs: subtotal 24.\n- Coupon 5: total 19.\n- Coupon 24: total 0.\n- Coupon 30: total 0.\n- Empty cart with coupon 5: total 0.\n")
    elif stage == "implement":
        if control["mode"] == "running":
            await_continue(control)
        tool("Read", "checkout.py")
        pause(control)
        add_regression()
        target = Path("checkout.py")
        target.write_text(target.read_text().replace("return subtotal(items) - coupon", 'return max(Decimal("0"), subtotal(items) - coupon)'))
        if (Path(".natamux-task") / ".review-request").exists():
            text = target.read_text()
            if 'if coupon < 0:' not in text:
                target.write_text(text.replace('    return max(Decimal("0"), subtotal(items) - coupon)',
                                              '    if coupon < 0:\n        raise ValueError("Coupon cannot be negative")\n    return max(Decimal("0"), subtotal(items) - coupon)'))
        tool("Edit", "checkout.py")
        hook("PostToolUse", tool_name="Edit", tool_input={"file_path": "checkout.py"})
        pause(control)
        tests()
        print("The code and regression tests remain uncommitted for the Changes view.", flush=True)
    elif stage == "agent-review":
        if not review(control):
            hook("Stop")
            return
    else:
        raise RuntimeError("Unknown demo stage: " + stage)
    pause(control)
    tool("Bash", "natamux done --stage " + stage, "command")
    subprocess.run(["natamux", "done", "--stage", stage], check=True)
    hook("Stop")
    print("\nWaiting for the next stage prompt.\n", flush=True)


def review(control):
    tool("Bash", "git diff --stat", "command")
    subprocess.run(["/usr/bin/git", "diff", "--stat"], check=True)
    marker = Path(".natamux-task") / ".review-request"
    if not marker.exists():
        negative_coupon_case()
        tests(expect_failure=True)
        reason = "checkout.py accepts a negative coupon and increases the total. Reject negative coupons with ValueError."
        marker.write_text(reason)
        document("review.md", "# Review\n\n## Findings\n- " + reason + "\n")
        pause(control)
        tool("Bash", "natamux stage back " + TASK + " --to implement", "command")
        subprocess.run(["natamux", "stage", "back", TASK, "--to", "implement", "--reason", reason], check=True)
        print("The real regression failed. Sent back to Implement with one finding.", flush=True)
        return False
    tests()
    document("review.md", "# Review\n\n## Findings\nNone. The clamp preserves the subtotal and small-coupon cases; negative coupons are rejected. All seven fixture tests passed in the real terminal.\n")
    return True


def main():
    if not TASK or not (ROOT / "capture.json").exists():
        raise RuntimeError("Run only inside a prepared native capture task")
    current = Path.cwd().resolve()
    if not current.is_relative_to((ROOT / "worktrees").resolve()):
        raise RuntimeError("Demo agents may only edit this capture's worktrees")
    control = json.loads((ROOT / "controls" / (TASK + ".json")).read_text())
    print("\033[2J\033[H\033[1mNatamux scripted demo agent\033[0m", flush=True)
    print(f"{TASK} · Shell {TERMINAL}\nReal files, tests and stage reports. No model calls.\n", flush=True)
    previous = termios.tcgetattr(sys.stdin)
    quiet = list(previous)
    quiet[3] &= ~termios.ECHO
    termios.tcsetattr(sys.stdin, termios.TCSANOW, quiet)
    try:
        prompt = sys.argv[-1] if len(sys.argv) > 1 else read_prompt()
        while prompt is not None:
            run_stage(prompt, control)
            prompt = read_prompt()
    finally:
        termios.tcsetattr(sys.stdin, termios.TCSANOW, previous)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print("Demo agent stopped: " + str(error), file=sys.stderr)
        sys.exit(1)
