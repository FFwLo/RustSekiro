//! The collision grid's cells: triangles by 2 m cell over the horizontal plane, then by who
//! collides with them (a layer per mask), then by height band, so a query near one height looks
//! only at the triangles that reach it and that it can collide with. Hot 4 m cells in m11_00 held
//! 3,000 to 7,400 triangles from several storeys, and a floor under a point needs only the few
//! whose footprint holds it (#163).
//!
//! A cell keeps the floors (faces flat enough to stand on, [`Column::floors`]) apart from the
//! rest: floor queries read only those, scanning downwards from the feet, and every other query
//! reads both. A floor is filed in every cell its box reaches padded by [`slack`], so a floor
//! query reads only the cell holding its point ([`super::filed_cells`]). Each kind keeps all its lists in one exactly sized block ([`Lists`]) that has
//! only the bands holding a triangle, so a cell spanning a cliff's height costs no more than one
//! on flat ground.
//!
//! A triangle reaching several bands is filed in each, in a part of each band that says whether
//! the band is its first in the kind's scan order: the lowest for the rest, the highest for the
//! floors ([`FIRST`]). A query reading bands `lo` to `hi` reads every part ([`ALL`]) of the
//! band it starts at and only first parts of the others ([`Layer::once`]), so it meets each
//! triangle once without working out where it starts. A triangle reaching more than
//! [`TALL_BANDS`] bands is filed once instead, in its layer's tall list.
//!
//! Each list keeps its triangles in triangle order, as the queries that depend on the order
//! need ([`super::CollisionWorld::gather_slide`] sorts what it gathers from several lists).

use std::ops::Range;

/// The height of a band. A power of two, so [`band_of`] and a band's bottom
/// (`band as f32 * BAND`) are exact.
pub(super) const BAND: f32 = 2.0;

/// The band holding height `y`.
pub(super) fn band_of(y: f32) -> i32 {
    (y * (1.0 / BAND)).floor() as i32
}

/// How far past its corners' box a floor query can read a height off a triangle spanning
/// `extent` (its largest side of that box) at coordinates up to `magnitude`: more than
/// [`super::height_on`]'s barycentric slack of 1e-5 per corner and float roundoff allow.
pub(super) fn slack(extent: f32, magnitude: f32) -> f32 {
    0.02 + 1e-4 * extent + 1e-6 * magnitude
}

/// The first and last band a triangle whose corners span heights `lo` to `hi` is filed in,
/// padded by [`slack`], so every height a floor query reads off the triangle lies within them.
pub(super) fn bands_of(lo: f32, hi: f32) -> (i32, i32) {
    let pad = slack(hi - lo, lo.abs().max(hi.abs()));
    (band_of(lo - pad), band_of(hi + pad))
}

/// The most bands a triangle is filed in band by band; one reaching more goes in its layer's
/// tall list ([`Layer::tall`]) instead, read whatever the height. A few huge faces span 100 m
/// and more (m25 has 168k such cell entries).
pub(super) const TALL_BANDS: i32 = 8;

/// Whether hit material `m` is a water surface's ([`super::View::water_surface`],
/// [`crate::body::water`]).
pub(super) fn water_material(m: u32) -> bool {
    crate::body::water(m)
}

/// The parts of a band: the triangles whose first band (in the lists' scan order) this is,
/// then those continuing from the band before.
const PARTS: usize = 2;
/// The triangles whose first band this is.
pub(super) const FIRST: Range<usize> = 0..1;
/// Every triangle of a band.
pub(super) const ALL: Range<usize> = 0..2;

/// One cell's lists of one kind, as one block of words. `DOWN` lists file each triangle's
/// [`FIRST`] part in its highest band, the others in its lowest.
///
/// - the layer count `L`, then per layer its mask and how many bands it has;
/// - the bands each layer has, lowest first, layer by layer: only those holding a triangle;
/// - the parts' starts, two for each layer's tall list ([`TALL_BANDS`]) then two for each of
///   its bands, layer by layer, and the end, as offsets into
/// - the triangles of every part, in that order.
pub(super) struct Lists<const DOWN: bool>(Box<[u32]>);

impl<const DOWN: bool> Default for Lists<DOWN> {
    /// No layers.
    fn default() -> Self {
        Self(Box::new([0]))
    }
}

/// One layer of a cell: the triangles those `mask` holds collide with
/// ([`super::CollisionWorld::live`]).
#[derive(Clone, Copy)]
pub(super) struct Layer<'a, const DOWN: bool> {
    pub mask: u8,
    /// The bands it has, lowest first.
    bands: &'a [u32],
    /// Its tall list's and bands' parts' starts and the end of its last part.
    starts: &'a [u32],
    /// The whole cell's triangles.
    entries: &'a [u32],
}

impl<'a, const DOWN: bool> Layer<'a, DOWN> {
    /// The `i`th band it has.
    fn band(&self, i: usize) -> i32 {
        self.bands[i] as i32
    }

    /// Its lowest and highest band, `None` when it has only a tall list.
    pub fn span(&self) -> Option<(i32, i32)> {
        let last = self.bands.len().checked_sub(1)?;
        Some((self.band(0), self.band(last)))
    }

    /// The highest band it has below `band`.
    pub fn below(&self, band: i32) -> Option<i32> {
        let i = self.bands.partition_point(|&b| (b as i32) < band);
        i.checked_sub(1).map(|i| self.band(i))
    }

    /// The triangles of parts `parts` of band `band`, none for a band it does not have.
    pub fn parts(&self, band: i32, parts: Range<usize>) -> &'a [u32] {
        match self.bands.binary_search_by_key(&band, |&b| b as i32) {
            Ok(i) => self.slot((i + 1) * PARTS, parts),
            Err(_) => &[],
        }
    }

    /// The triangles of the tall list: those filed in more than [`TALL_BANDS`] bands, in no
    /// band.
    pub fn tall(&self) -> &'a [u32] {
        self.slot(0, ALL)
    }

    /// The lists holding, each once, the triangles filed in bands `lo` to `hi` (and the tall
    /// list): every part of the band the scan starts at (`hi` for `DOWN` lists, `lo` for the
    /// rest) and the [`FIRST`] parts of the others. A band the layer does not have holds
    /// nothing, nor does the continuing part of the first band it has past it.
    pub fn once(&self, lo: i32, hi: i32) -> impl Iterator<Item = &'a [u32]> + use<'a, DOWN> {
        let layer = *self;
        let from = self.bands.partition_point(|&b| (b as i32) < lo);
        let to = self.bands.partition_point(|&b| (b as i32) <= hi);
        let start = if DOWN { hi } else { lo };
        std::iter::once(self.tall()).chain((from..to).map(move |i| {
            let parts = if layer.band(i) == start { ALL } else { FIRST };
            layer.slot((i + 1) * PARTS, parts)
        }))
    }

    fn slot(&self, slot: usize, parts: Range<usize>) -> &'a [u32] {
        let [from, to] = [parts.start, parts.end].map(|p| self.starts[slot + p] as usize);
        &self.entries[from..to]
    }

    /// How many triangles it files, with repeats.
    fn len(&self) -> usize {
        (self.starts[self.starts.len() - 1] - self.starts[0]) as usize
    }
}

/// A layer of a cell being packed ([`Lists::pack`]): its mask, its bands, lowest first, and the
/// layers whose triangles it holds, each with the offset its triangles are numbered from.
type Packing<'a, const DOWN: bool> = (u8, Vec<i32>, Vec<(Layer<'a, DOWN>, u32)>);

impl<const DOWN: bool> Lists<DOWN> {
    /// Where the bands, the starts and the triangles begin.
    fn offsets(&self) -> [usize; 3] {
        let words = &self.0;
        let count = words[0] as usize;
        let bands: usize = (0..count).map(|i| words[2 + 2 * i] as usize).sum();
        let bands_at = 1 + 2 * count;
        let starts_at = bands_at + bands;
        [bands_at, starts_at, starts_at + (bands + count) * PARTS + 1]
    }

    /// The layers, each once.
    pub fn layers(&self) -> impl Iterator<Item = Layer<'_, DOWN>> + Clone {
        let words = &self.0;
        let [mut band, mut slot, entries_at] = self.offsets();
        let entries = &words[entries_at.min(words.len())..];
        (0..words[0] as usize).map(move |i| {
            let n = words[2 + 2 * i] as usize;
            let layer = Layer {
                mask: words[1 + 2 * i] as u8,
                bands: &words[band..band + n],
                starts: &words[slot..=slot + (n + 1) * PARTS],
                entries,
            };
            band += n;
            slot += (n + 1) * PARTS;
            layer
        })
    }

    /// The layers `who` collides with ([`super::CollisionWorld::live`] is false for any triangle
    /// of the others).
    pub fn layers_for(&self, who: u8) -> impl Iterator<Item = Layer<'_, DOWN>> + Clone {
        self.layers().filter(move |l| l.mask & who != 0)
    }

    /// The lowest and highest band of the layers `who` collides with, their tall lists aside.
    pub fn span(&self, who: u8) -> Option<(i32, i32)> {
        self.layers_for(who)
            .filter_map(|l| l.span())
            .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
    }

    /// Every triangle filed here, with repeats.
    #[cfg(test)]
    pub fn entries(&self) -> &[u32] {
        &self.0[self.offsets()[2].min(self.0.len())..]
    }

    /// Heap bytes.
    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.0.len() * 4
    }

    /// The cell holding `layers`, each `(mask, bands, sources)`: part `k` of band `b` (and of
    /// the tall list) is the triangles of part `k` of band `b` (of the tall list) of each source
    /// layer in turn, each numbered from its offset on.
    fn pack(layers: &[Packing<'_, DOWN>]) -> Self {
        let bands: usize = layers.iter().map(|l| l.1.len()).sum();
        let total: usize = layers.iter().flat_map(|l| &l.2).map(|(s, _)| s.len()).sum();
        let starts = (bands + layers.len()) * PARTS + 1;
        let mut words = Vec::with_capacity(1 + 2 * layers.len() + bands + starts + total);
        words.push(layers.len() as u32);
        for (mask, bands, _) in layers {
            words.extend([u32::from(*mask), bands.len() as u32]);
        }
        for (_, bands, _) in layers {
            words.extend(bands.iter().map(|&b| b as u32));
        }
        let mut entries = Vec::with_capacity(total);
        for (_, bands, sources) in layers {
            // The tall list (`None`), then the bands.
            for band in std::iter::once(None).chain(bands.iter().copied().map(Some)) {
                for k in 0..PARTS {
                    words.push(entries.len() as u32);
                    for (source, offset) in sources {
                        let list = match band {
                            Some(band) => source.parts(band, k..k + 1),
                            None => source.slot(0, k..k + 1),
                        };
                        entries.extend(list.iter().map(|t| t + offset));
                    }
                }
            }
        }
        words.push(entries.len() as u32);
        words.extend(entries);
        Self(words.into_boxed_slice())
    }

    /// This cell with `other`'s triangles, numbered from `offset` on here, moved to layer
    /// `mask`: after its triangles when it has that layer already.
    pub fn merge(&self, other: &Self, offset: u32, mask: u8) -> Self {
        let ids = |l: &Layer<'_, DOWN>| l.bands.iter().map(|&b| b as i32).collect::<Vec<_>>();
        let mut layers: Vec<_> = self
            .layers()
            .map(|l| (l.mask, ids(&l), vec![(l, 0)]))
            .collect();
        for l in other.layers() {
            match layers.iter_mut().find(|(m, ..)| *m == mask) {
                Some((_, bands, sources)) => {
                    bands.extend(ids(&l));
                    bands.sort_unstable();
                    bands.dedup();
                    sources.push((l, offset));
                }
                None => layers.push((mask, ids(&l), vec![(l, offset)])),
            }
        }
        Self::pack(&layers)
    }
}

/// A cell's lists of one kind while a mesh is filed ([`Pending::file`]), packed once it is all
/// filed.
#[derive(Default)]
pub(super) struct Pending<const DOWN: bool> {
    /// `(band, part << 28 | triangle)`, in triangle order; [`Pending::TALL`] for the tall list.
    entries: Vec<(i32, u32)>,
}

impl<const DOWN: bool> Pending<DOWN> {
    /// The band of the tall list's entries.
    const TALL: i32 = i32::MIN;

    /// Files triangle `t` (below 2^28, [`super::CollisionWorld::build`]), whose first band is
    /// `first` and last `last`, in each, or in the tall list when there are more than
    /// [`TALL_BANDS`].
    pub fn file(&mut self, first: i32, last: i32, t: u32) {
        if last - first >= TALL_BANDS {
            self.entries.push((Self::TALL, t));
            return;
        }
        let start = if DOWN { last } else { first };
        for band in first..=last {
            self.entries
                .push((band, u32::from(band != start) << 28 | t));
        }
    }

    /// The cell's lists, in one layer for those `mask` holds, each list in triangle order.
    pub fn pack(self, mask: u8) -> Lists<DOWN> {
        if self.entries.is_empty() {
            return Lists::default();
        }
        let mut bands: Vec<i32> = self
            .entries
            .iter()
            .map(|e| e.0)
            .filter(|&b| b != Self::TALL)
            .collect();
        bands.sort_unstable();
        bands.dedup();
        // Each band's slot (the tall list's is 0), by its offset from the lowest.
        let base = bands.first().copied().unwrap_or(0);
        let mut index = vec![0; bands.last().map_or(0, |&top| (top - base + 1) as usize)];
        for (i, &band) in bands.iter().enumerate() {
            index[(band - base) as usize] = i + 1;
        }
        let slot = |(band, word): (i32, u32)| {
            let i = if band == Self::TALL {
                0
            } else {
                index[(band - base) as usize]
            };
            i * PARTS + (word >> 28) as usize
        };
        // A counting sort by slot, which keeps each slot's triangles in triangle order.
        let starts_at = 3 + bands.len();
        let entries_at = starts_at + (bands.len() + 1) * PARTS + 1;
        let mut words = vec![0u32; entries_at + self.entries.len()];
        words[..3].copy_from_slice(&[1, u32::from(mask), bands.len() as u32]);
        for (word, &band) in words[3..starts_at].iter_mut().zip(&bands) {
            *word = band as u32;
        }
        for &e in &self.entries {
            words[starts_at + slot(e) + 1] += 1;
        }
        for i in starts_at + 1..entries_at {
            words[i] += words[i - 1];
        }
        let mut next = words[starts_at..entries_at - 1].to_vec();
        for e in self.entries {
            let at = &mut next[slot(e)];
            words[entries_at + *at as usize] = e.1 & ((1 << 28) - 1);
            *at += 1;
        }
        Lists(words.into_boxed_slice())
    }
}

/// One cell of the grid ([`super::CELL`] square).
#[derive(Default)]
pub(super) struct Column {
    /// The triangles that are not floors, each in the cells its corners' box reaches.
    pub rest: Lists<false>,
    /// The triangles that would be floors (as [`super::CollisionWorld::walkable`] asks, less
    /// whether they are live), each in the cells its corners' box, padded by [`slack`], reaches,
    /// so a floor query finds every floor it can read a height off.
    pub floors: Lists<true>,
    /// The triangles with a water material, at any height and in any layer
    /// ([`super::View::water_surface`]).
    pub water: Box<[u32]>,
}

impl Column {
    /// This cell with `other`'s triangles, numbered from `offset` on here, in layer `mask`.
    pub fn merge(&self, other: &Column, offset: u32, mask: u8) -> Column {
        Column {
            rest: self.rest.merge(&other.rest, offset, mask),
            floors: self.floors.merge(&other.floors, offset, mask),
            water: self
                .water
                .iter()
                .copied()
                .chain(other.water.iter().map(|t| t + offset))
                .collect(),
        }
    }

    /// The lists of both kinds `who` collides with holding, each once, the triangles filed in
    /// bands `lo` to `hi`.
    pub fn once(&self, who: u8, lo: i32, hi: i32) -> impl Iterator<Item = &[u32]> {
        let rest = self.rest.layers_for(who).flat_map(move |l| l.once(lo, hi));
        let floors = self
            .floors
            .layers_for(who)
            .flat_map(move |l| l.once(lo, hi));
        rest.chain(floors)
    }
}
