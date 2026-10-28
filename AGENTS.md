# agents.md

This file provides agents with guidance when working in this project.

## Code Style Guidelines
- Use type hints frequently
- Use descriptive variable names
- Document complex functions with docstrings

## Workflow
- Work like a scientist. Think step by step and seek evidence before declaring success
- Be clear and concise in your writing, without flowery language
- Create a PLAN.md file to plan and track your work
    - Example
    ```
    # <Issue title>
    ## Description
    <The work requires that I do X and achieve Y>
    ## Steps
    ### Step 1: <thing to do>
    - [ ] <Task 1>
        - [ ] <Subtask 1>
    - [ ] <Task 2>
    ### Step 2: <next thing>
    - [ ] <Task 1>
    ...
    ### Notes
    ```
    - Update the PLAN.md as you complete tasks

## Documentation
When a command-line flag, subcommand, default, accepted range, or printed error text changes, update `README.md` and `README-zh.md` in the same edit. `README.md` is English. `README-zh.md` is the complete Chinese translation of the same sections, commands, defaults, and limits. Keep the section order aligned. The edit is done when both files describe the same CLI.
