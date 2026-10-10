# Wave smoke: write one note

Create the file `engineering/wave-smoke.md`. It has exactly two lines. Both lines are ASCII.
The first line says that the file is a wave smoke note.
The second line names spec 0025, the wave execution spec.

Change no other file. Do not commit. Do not push. Do not open a pull request.

Acceptance: `wc -l engineering/wave-smoke.md` prints 2, and `grep -c 0025 engineering/wave-smoke.md` prints 1.
