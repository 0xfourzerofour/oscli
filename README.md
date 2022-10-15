### OSCLI - Real time audio visualisation using WGPU

The implementation is quite simple right now, however I will be planning on continuing the project to visualise the frequency spectrum as well. 

The current implementation only works with mp3 files using [minimp3-rs](https://github.com/germangb/minimp3-rs)

# Instructions

```
cargo run --release

```

drag your mp3 file into the window.

# controls

spacebar - play
p - pause
up arrow - skip 1 second

# future work

- Allow WAV files using hound
- Make the vertex buffer much leaner by interpolating the ring-buffer instead of just passing raw PCM data.
- use FFT to derive the freqency domain
- once FFT is implemented, render the audio in 3D space
- zoom functionality


# Demo


https://user-images.githubusercontent.com/36560907/195976444-5775e6ee-2acd-49da-ad6f-1010f9fe631b.mov

