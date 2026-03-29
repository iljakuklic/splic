#!/usr/bin/env -S python3
"""Fetch unresolved PR review threads for the current branch's PR."""

import json
import subprocess
import sys

GRAPHQL_QUERY = """
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      number
      title
      reviewThreads(first: 100) {
        nodes {
          isResolved
          comments(first: 100) {
            nodes {
              databaseId
              author { login }
              path
              line
              originalLine
              body
              replyTo { databaseId }
            }
          }
        }
      }
    }
  }
}
"""


def run(cmd):
    result = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return result.stdout.strip()


def main():
    # Get current PR info
    try:
        pr_info = json.loads(run(["gh", "pr", "view", "--json", "number,title,headRepository"]))
    except subprocess.CalledProcessError:
        print("ERROR: No open PR found for the current branch.", file=sys.stderr)
        sys.exit(1)

    pr_number = pr_info["number"]
    pr_title = pr_info["title"]

    # Get repo owner/name
    repo_info = json.loads(run(["gh", "repo", "view", "--json", "nameWithOwner"]))
    owner, repo = repo_info["nameWithOwner"].split("/", 1)

    # Fetch review threads via GraphQL
    variables = json.dumps({"owner": owner, "repo": repo, "number": pr_number})
    gql_result = run([
        "gh", "api", "graphql",
        "--field", f"query={GRAPHQL_QUERY}",
        "--field", f"owner={owner}",
        "--field", f"repo={repo}",
        "--field", f"number={pr_number}",
    ])
    data = json.loads(gql_result)

    threads = data["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
    unresolved = [t for t in threads if not t["isResolved"]]

    if not unresolved:
        print(f"No unresolved review threads on PR #{pr_number}.")
        return

    print(f"=== UNRESOLVED PR REVIEW COMMENTS ===")
    print(f"PR #{pr_number}: {pr_title}")
    print(f"Repository: {owner}/{repo}")
    print(f"Unresolved threads: {len(unresolved)}")
    print()

    for idx, thread in enumerate(unresolved, 1):
        comments = thread["comments"]["nodes"]
        root = comments[0]
        replies = comments[1:]

        # Prefer line over originalLine (originalLine is the position at time of comment)
        location = root.get("line") or root.get("originalLine")
        location_str = f"line {location}" if location else "file level"

        print(f"--- Thread {idx} ---")
        print(f"File:       {root['path']} ({location_str})")
        print(f"Comment ID: {root['databaseId']}")
        print(f"Author:     {root['author']['login']}")
        print(f"Body:")
        for line in root["body"].splitlines():
            print(f"  {line}")

        for reply in replies:
            print()
            print(f"  [Reply — ID: {reply['databaseId']}, Author: {reply['author']['login']}]")
            for line in reply["body"].splitlines():
                print(f"    {line}")

        print()


if __name__ == "__main__":
    main()
