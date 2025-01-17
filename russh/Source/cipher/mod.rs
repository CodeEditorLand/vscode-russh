// Copyright 2016 Pierre-Étienne Meunier
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This module exports cipher names for use with [Preferred].
#[cfg(feature = "rs-crypto")]
use std::marker::PhantomData;
use std::{collections::HashMap, fmt::Debug, num::Wrapping};

use byteorder::{BigEndian, ByteOrder};
use log::debug;
use once_cell::sync::Lazy;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{Error, mac::MacAlgorithm, sshbuffer::SSHBuffer};

pub(crate) mod clear;

#[cfg(feature = "openssl")]
pub(crate) mod aes_openssh;
#[cfg(feature = "rs-crypto")]
pub(crate) mod block;
#[cfg(feature = "rs-crypto")]
pub(crate) mod chacha20poly1305;
#[cfg(feature = "rs-crypto")]
pub(crate) mod gcm;

#[cfg(feature = "rs-crypto")]
use block::SshBlockCipher;
#[cfg(feature = "rs-crypto")]
use chacha20poly1305::SshChacha20Poly1305Cipher;
use clear::Clear;
#[cfg(feature = "rs-crypto")]
use gcm::GcmCipher;

pub(crate) trait Cipher {
	fn needs_mac(&self) -> bool { false }

	fn key_len(&self) -> usize;

	fn nonce_len(&self) -> usize { 0 }

	fn make_opening_key(
		&self,
		key:&[u8],
		nonce:&[u8],
		mac_key:&[u8],
		mac:&dyn MacAlgorithm,
	) -> Result<Box<dyn OpeningKey + Send>, Error>;

	fn make_sealing_key(
		&self,
		key:&[u8],
		nonce:&[u8],
		mac_key:&[u8],
		mac:&dyn MacAlgorithm,
	) -> Result<Box<dyn SealingKey + Send>, Error>;
}

/// `clear`
pub const CLEAR:Name = Name("clear");
/// `aes128-ctr`
pub const AES_128_CTR:Name = Name("aes128-ctr");
/// `aes192-ctr`
pub const AES_192_CTR:Name = Name("aes192-ctr");
/// `aes256-ctr`
pub const AES_256_CTR:Name = Name("aes256-ctr");
/// `aes256-gcm@openssh.com`
pub const AES_256_GCM:Name = Name("aes256-gcm@openssh.com");
/// `chacha20-poly1305@openssh.com`
pub const CHACHA20_POLY1305:Name = Name("chacha20-poly1305@openssh.com");
/// `none`
pub const NONE:Name = Name("none");

static _CLEAR:Clear = Clear {};

#[cfg(all(feature = "openssl", not(feature = "rs-crypto")))]
static _AES_128_CTR:aes_openssh::AesSshCipher =
	aes_openssh::AesSshCipher(openssl::cipher::Cipher::aes_128_ctr);
#[cfg(feature = "rs-crypto")]
static _AES_128_CTR:SshBlockCipher<ctr::Ctr128BE<aes::Aes128>> = SshBlockCipher(PhantomData);

#[cfg(all(feature = "openssl", not(feature = "rs-crypto")))]
static _AES_192_CTR:aes_openssh::AesSshCipher =
	aes_openssh::AesSshCipher(openssl::cipher::Cipher::aes_192_ctr);
#[cfg(feature = "rs-crypto")]
static _AES_192_CTR:SshBlockCipher<ctr::Ctr128BE<aes::Aes192>> = SshBlockCipher(PhantomData);

#[cfg(all(feature = "openssl", not(feature = "rs-crypto")))]
static _AES_256_CTR:aes_openssh::AesSshCipher =
	aes_openssh::AesSshCipher(openssl::cipher::Cipher::aes_256_ctr);
#[cfg(feature = "rs-crypto")]
static _AES_256_CTR:SshBlockCipher<ctr::Ctr128BE<aes::Aes256>> = SshBlockCipher(PhantomData);

#[cfg(feature = "rs-crypto")]
static _AES_256_GCM:GcmCipher = GcmCipher {};

#[cfg(feature = "rs-crypto")]
static _CHACHA20_POLY1305:SshChacha20Poly1305Cipher = SshChacha20Poly1305Cipher {};

pub(crate) static CIPHERS:Lazy<HashMap<&'static Name, &(dyn Cipher + Send + Sync)>> =
	Lazy::new(|| {
		let mut h:HashMap<&'static Name, &(dyn Cipher + Send + Sync)> = HashMap::new();

		h.insert(&CLEAR, &_CLEAR);

		h.insert(&NONE, &_CLEAR);

		h.insert(&AES_128_CTR, &_AES_128_CTR);

		h.insert(&AES_192_CTR, &_AES_192_CTR);

		h.insert(&AES_256_CTR, &_AES_256_CTR);
		#[cfg(feature = "rs-crypto")]
		h.insert(&AES_256_GCM, &_AES_256_GCM);
		#[cfg(feature = "rs-crypto")]
		h.insert(&CHACHA20_POLY1305, &_CHACHA20_POLY1305);

		h
	});

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct Name(&'static str);
impl AsRef<str> for Name {
	fn as_ref(&self) -> &str { self.0 }
}

pub(crate) struct CipherPair {
	pub local_to_remote:Box<dyn SealingKey + Send>,
	pub remote_to_local:Box<dyn OpeningKey + Send>,
}

impl Debug for CipherPair {
	fn fmt(&self, _:&mut std::fmt::Formatter) -> Result<(), std::fmt::Error> { Ok(()) }
}

pub(crate) trait OpeningKey {
	fn decrypt_packet_length(
		&self,
		seqn:u32,
		encrypted_packet_length:[u8; 4],
	) -> Result<[u8; 4], Error>;

	fn tag_len(&self) -> usize;

	fn open<'a>(
		&mut self,
		seqn:u32,
		ciphertext_in_plaintext_out:&'a mut [u8],
		tag:&[u8],
	) -> Result<&'a [u8], Error>;
}

pub(crate) trait SealingKey {
	fn padding_length(&self, plaintext:&[u8]) -> usize;

	fn fill_padding(&self, padding_out:&mut [u8]);

	fn tag_len(&self) -> usize;

	fn seal(&mut self, seqn:u32, plaintext_in_ciphertext_out:&mut [u8], tag_out:&mut [u8]);

	fn write(&mut self, payload:&[u8], buffer:&mut SSHBuffer) {
		// https://tools.ietf.org/html/rfc4253#section-6
		//
		// The variables `payload`, `packet_length` and `padding_length` refer
		// to the protocol fields of the same names.
		debug!("writing, seqn = {:?}", buffer.seqn.0);

		let padding_length = self.padding_length(payload);

		debug!("padding length {:?}", padding_length);

		let packet_length = PADDING_LENGTH_LEN + payload.len() + padding_length;

		debug!("packet_length {:?}", packet_length);

		let offset = buffer.buffer.len();

		// Maximum packet length:
		// https://tools.ietf.org/html/rfc4253#section-6.1
		assert!(packet_length <= std::u32::MAX as usize);

		buffer.buffer.push_u32_be(packet_length as u32);

		assert!(padding_length <= std::u8::MAX as usize);

		buffer.buffer.push(padding_length as u8);

		buffer.buffer.extend(payload);

		self.fill_padding(buffer.buffer.resize_mut(padding_length));

		buffer.buffer.resize_mut(self.tag_len());

		#[allow(clippy::indexing_slicing)] // length checked
		let (plaintext, tag) = buffer.buffer[offset..].split_at_mut(PACKET_LENGTH_LEN + packet_length);

		self.seal(buffer.seqn.0, plaintext, tag);

		buffer.bytes += payload.len();
		// Sequence numbers are on 32 bits and wrap.
		// https://tools.ietf.org/html/rfc4253#section-6.4
		buffer.seqn += Wrapping(1);
	}
}

pub(crate) async fn read<'a, R:AsyncRead + Unpin>(
	stream:&'a mut R,
	buffer:&'a mut SSHBuffer,
	cipher:&'a mut (dyn OpeningKey + Send),
) -> Result<usize, Error> {
	if buffer.len == 0 {
		let mut len = [0; 4];

		stream.read_exact(&mut len).await?;

		debug!("reading, len = {:?}", len);
		{
			let seqn = buffer.seqn.0;

			buffer.buffer.clear();

			buffer.buffer.extend(&len);

			debug!("reading, seqn = {:?}", seqn);

			let len = cipher.decrypt_packet_length(seqn, len)?;

			buffer.len = BigEndian::read_u32(&len) as usize + cipher.tag_len();

			debug!("reading, clear len = {:?}", buffer.len);
		}
	}

	buffer.buffer.resize(buffer.len + 4);

	debug!("read_exact {:?}", buffer.len + 4);
	#[allow(clippy::indexing_slicing)] // length checked
	stream.read_exact(&mut buffer.buffer[4..]).await?;

	debug!("read_exact done");

	let seqn = buffer.seqn.0;

	let ciphertext_len = buffer.buffer.len() - cipher.tag_len();

	let (ciphertext, tag) = buffer.buffer.split_at_mut(ciphertext_len);

	let plaintext = cipher.open(seqn, ciphertext, tag)?;

	let padding_length = *plaintext.first().to_owned().unwrap_or(&0) as usize;

	debug!("reading, padding_length {:?}", padding_length);

	let plaintext_end =
		plaintext.len().checked_sub(padding_length).ok_or(Error::IndexOutOfBounds)?;

	// Sequence numbers are on 32 bits and wrap.
	// https://tools.ietf.org/html/rfc4253#section-6.4
	buffer.seqn += Wrapping(1);

	buffer.len = 0;

	// Remove the padding
	buffer.buffer.resize(plaintext_end + 4);

	Ok(plaintext_end + 4)
}

pub(crate) const PACKET_LENGTH_LEN:usize = 4;

const MINIMUM_PACKET_LEN:usize = 16;

const PADDING_LENGTH_LEN:usize = 1;
