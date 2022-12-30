use core::ffi::CStr;

use super::{IfCtx, IfInfo, SocketMmapOffsets};
use crate::xdp::{XdpMmapOffsets, XdpMmapOffsetsV1};

impl IfInfo {
    pub fn invalid() -> Self {
        IfInfo {
            ctx: IfCtx {
                ifindex: 0,
                queue_id: 0,
                netnscookie: 0,
            },
            ifname: [b'\0' as libc::c_char; libc::IFNAMSIZ],
        }
    }

    pub fn from_name(&mut self, st: &CStr) -> Result<(), libc::c_int> {
        let bytes = st.to_bytes_with_nul();
        assert!(bytes.len() <= self.ifname.len());
        let bytes = unsafe { &*(bytes as *const _ as *const [libc::c_char]) };

        let index = unsafe { libc::if_nametoindex(st.as_ptr()) };
        if index == 0 {
            return Err(unsafe { *libc::__errno_location() });
        }

        self.ctx.ifindex = index;
        self.ctx.queue_id = 0;
        self.ctx.netnscookie = 0;
        self.ifname[..bytes.len()].copy_from_slice(bytes);

        Ok(())
    }

    pub fn from_ifindex(&mut self, index: libc::c_uint) -> Result<(), libc::c_int> {
        let err = unsafe { libc::if_indextoname(index, self.ifname.as_mut_ptr()) };

        if err.is_null() {
            return Err(unsafe { *libc::__errno_location() });
        }

        Ok(())
    }
}

impl SocketMmapOffsets {
    const OPT_V1: libc::socklen_t = core::mem::size_of::<XdpMmapOffsetsV1>() as libc::socklen_t;
    const OPT_LATEST: libc::socklen_t = core::mem::size_of::<XdpMmapOffsets>() as libc::socklen_t;

    pub fn new(fd: libc::c_int) -> Result<Self, libc::c_int> {
        let mut this = SocketMmapOffsets { inner: Default::default() };
        this.set_from_fd(fd)?;
        Ok(this)
    }

    pub fn set_from_fd(&mut self, fd: libc::c_int) -> Result<(), libc::c_int> {
        use crate::xdp::{XdpRingOffsets, XdpRingOffsetsV1};

        // The flags was implicit, based on the consumer.
        fn fixup_v1(v1: XdpRingOffsetsV1) -> XdpRingOffsets {
            XdpRingOffsets {
                producer: v1.producer,
                consumer: v1.consumer,
                desc: v1.desc,
                flags: v1.consumer + core::mem::size_of::<u32>() as u64,
            }
        }

        union Offsets {
            v1: XdpMmapOffsetsV1,
            latest: XdpMmapOffsets,
            init: (),
        }

        let mut off = Offsets { init: () };
        let mut optlen: libc::socklen_t = core::mem::size_of_val(&off) as libc::socklen_t;

        let err = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_XDP,
                super::XskUmem::XDP_MMAP_OFFSETS,
                (&mut off) as *mut _ as *mut libc::c_void,
                &mut optlen,
            )
        };

        if err != 0 {
            return Err(err);
        }

        match optlen {
            Self::OPT_V1 => {
                let v1 = unsafe { off.v1 };

                self.inner = XdpMmapOffsets {
                    rx: fixup_v1(v1.rx),
                    tx: fixup_v1(v1.tx),
                    fr: fixup_v1(v1.fr),
                    cr: fixup_v1(v1.cr),
                };

                Ok(())
            }
            Self::OPT_LATEST => {
                self.inner = unsafe { off.latest };
                Ok(())
            }
            _ => Err(-libc::EINVAL),
        }
    }
}
