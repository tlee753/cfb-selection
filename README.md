# CFB Selection Visualizer
- quick and dirty program to visualize college football rankings by wins and losses
- top 25 is show on the left panel, the selected teams respective schedule is shown on the right
- wins are green, losses are red, and blue means an upcoming ranked game
- just a fun tool built mostly to explore adwaita/gtk GUI development in rust
- and to test out cross compilation for windows (mingw)
- this code is not great and should be re-written, but the application mattered far less to me than experimenting with gtk4

### Demo
![Texas](examples/texas-demo.png)
![Week 12](examples/week-12.png)

<video controls>
    <source src="examples/pre-conference-championship.mov" type="video/quicktime">
    Your browser does not support the video tag.
</video>

### How to Run
- simple `cargo run` should do the trick ;)

### Data Source
- All data is from [Sports Reference](https://www.sports-reference.com/cfb)
- Example: `https://www.sports-reference.com/cfb/schools/indiana/2025-schedule.html`

### Packaging
- [Windows Cross Compilation Example](https://github.com/dagit/annelid)

### Future (unlikely, but who knows)
- [ ] color code based on strength of victory
- [ ] fix miami-fl vs miami-oh bug and miami-fl being considered ranked due to paranthesis
- [ ] clean up sidebar
    - [ ] add ranks
- [ ] clean up schedule
    - [ ] add date
    - [ ] add css on scores
- [ ] add color key
- [ ] port to windows from linux

### Windows Cross Compilation
```bash
cargo build --target x86_64-pc-windows-gnu --release
```
