#!/usr/bin/env python3
"""Fails when a traced program reached past loopback.

Usage: strace -f -e trace=connect,sendto,sendmsg -o <file> <program>; net-trace.py <file>

Reads the strace output and looks at every IPv4 and IPv6 address a process connected or sent
to. Loopback is fine. Any other address fails, and so does port 53 on any address: a DNS lookup
means the program was about to reach a named host. Unix sockets and netlink are not network.
"""

import ipaddress
import re
import sys

V4 = re.compile(r'sa_family=AF_INET, sin_port=htons\((\d+)\), sin_addr=inet_addr\("([^"]+)"\)')
V6 = re.compile(r'sa_family=AF_INET6, sin6_port=htons\((\d+)\),.*?inet_pton\(AF_INET6, "([^"]+)"')


def loopback(text):
    address = ipaddress.ip_address(text)
    mapped = getattr(address, "ipv4_mapped", None)
    return (mapped or address).is_loopback


def check(lines):
    """(problems, number of loopback addresses seen)."""
    problems, local = [], 0
    for line in lines:
        for port, address in V4.findall(line) + V6.findall(line):
            if port == "53":
                problems.append(f"DNS lookup through {address}: {line.strip()}")
            elif not loopback(address):
                problems.append(f"connection to {address} port {port}: {line.strip()}")
            else:
                local += 1
    return problems, local


def main():
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    with open(sys.argv[1], errors="replace") as trace:
        problems, local = check(trace)
    for problem in problems:
        print(f"net-trace: {problem}", file=sys.stderr)
    if not problems and local == 0:
        # The app always talks to its daemon on loopback: none seen means the trace did not work.
        print("net-trace: no loopback connection in the trace, so it shows nothing", file=sys.stderr)
        return 1
    if not problems:
        print(f"net-trace: {local} loopback connections, nothing else")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
