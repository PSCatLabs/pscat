# #845: native ridged multifractal and eroded fBm (reverted, not planned)

#845 asked for two native operators, `ridgedmf2` and `erodedfbm2`, to
speed up the top two octave loops of the fractal_clouds profile. They
shipped in #851 and were reverted the same day, on 2026-10-26, at the
owner's direction:

> Fractal clouds asked for domain specific logic in the engine itself.
> We should push back on that.

Why: those loops encode one project's terrain recipe (Musgrave's ridged
multifractal weighting, Quilez's derivative-damped fBm, one octave-walk
and rotation convention). That recipe belongs in the user's own
PostScript, not in pscat. The engine keeps only the general noise
primitives from #734 (`noiseseed`, `gnoise2`/`gnoise3`, `vnoise2`/
`vnoise3`, `gnoise2d`, `fbm2`/`fbm3`, `vfbm2`/`vfbm3`, `ridged2`/
`ridged3`), which are unchanged and bit-identical.

If a similar request comes up (a native operator for one recipe), the
answer is the same: write the loop in PostScript on top of `gnoise2`,
`gnoise2d` and friends. `gnoise2d` returns the value and both partial
derivatives, which is all the eroded variant needs. If a primitive is
missing or too slow for a general reason, file that, not the recipe.
