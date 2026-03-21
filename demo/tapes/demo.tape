Output demo/tapes/demo.gif
Set FontSize 13
Set Width 1200
Set Height 900
Set Theme "Molokai"
Set Padding 15
Set TypingSpeed 20ms
Set Shell "bash"

# Hidden setup
Hide
Type "cd $(bash /Users/mike/github/sw-vibe-coding/agentrail-rs/demo/scripts/tape-setup.sh /Users/mike/github/sw-vibe-coding/agentrail-rs)"
Enter
Sleep 2s
Type "clear"
Enter
Sleep 500ms
Show

# Demo
Type "# AgentRail: Skill-guided agent prompting"
Enter
Sleep 1500ms
Type "# An agent starts a fresh context and runs 'agentrail next':"
Enter
Sleep 1s

Type "agentrail next"
Enter
Sleep 5s

Type ""
Enter
Type "# ^ Skills, failure modes, and past trajectories injected automatically"
Enter
Sleep 3s
Type "clear"
Enter
Sleep 300ms

Type "# The agent follows the skill procedure, then records the result:"
Enter
Sleep 1s

Type "agentrail begin"
Enter
Sleep 1s

Type "agentrail complete --summary 'Created project with edition 2024, clippy clean' --done"
Enter
Sleep 2s

Type "agentrail status"
Enter
Sleep 3s

Type ""
Enter
Type "# Skills turn agent amnesia into learned behavior."
Enter
Sleep 3s
