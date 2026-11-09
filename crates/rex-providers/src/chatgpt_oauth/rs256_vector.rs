//! An RS256 test vector made by an independent implementation (Python's
//! `cryptography`, 2048-bit key, PKCS#1 v1.5 with SHA-256), so the RS256 path
//! is checked against something other than this crate's own code.
pub const N: &str = "oJjuTFxezVYKMhIxKgsbGhYL3_kRx9E_YqQnIbJC5as0Ya42YXgYOiLNxaFV39dncVJudrBicCL5v-EFAjbE6PVOcB28A_iPDFLZnDXCsbsAs0l8MzleWgnb93Iry8xuBqUaIwRdQjC-wPHG4AgHX9Fc08g3Avk_V0ScQedz0GKfVE1_GQmAR4jaaiviUVe6yrTSNiuZRx67iA0SZPMYCNSP86R3tF08LqeKGoWzHAYSocd6vc6uz3j7Ydp3oqTc3hrPy5PCvj2vaz0_VPJ6E3Q_jIVnOiBi9X6KxmwrGG4o_fuqtf6yAi-k3caO7rcTPamJgCBEOEFbxS_0y0w8LQ";
pub const E: &str = "AQAB";
pub const MESSAGE: &str = "rex-rs256-test-vector";
pub const SIGNATURE: &str = "hu457iw9SfaH9GLLZI1d_njn0KhLFWlGcrUsDEieMnsgLlV_8IkTe5GCTmBJd8LxemQhQ5wB-BIEGYa7ue1c5LxfxazNzz25wAkJEVFlGK_mvwLfDgysF-bbrbR6slEV_1GjOwWMm6BlaoMJUDXr1GwxN_8Pw-fk5Rck-cKsELDuTAUr8WwYpB-wLA1g5jLrwEO6nOng4LVIgZqoSpVwCXzupLDrSLJEvcaNQdf-q5x1ujPUvg9icFrBs-1s-dY7tKW6qWmJijkkgy5N1ea5ZVTUEUqtL6P3Pk0Re3lHLTxxcugDXv6miywYyZSacsFIZb-SdglXWz2SFxF-ChL3vw";
