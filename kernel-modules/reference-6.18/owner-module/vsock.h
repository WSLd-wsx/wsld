/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef WSX_VIRTIO_MSG_VSOCK_H
#define WSX_VIRTIO_MSG_VSOCK_H
#include "virtio_msg_internal.h"

struct vmsg_vsock;
struct pci_dev;

/* Offline integration library: no module init/probe or device registration.
 * The owner retains vmdev until shutdown, all transfer callers and callbacks
 * have drained, destroy has returned, and the virtio device references retire.
 * Copy these operations into the owner's existing table, retaining its release
 * callback; connect supplies bus_data but never owns/frees/registers vmdev.
 * The owner must select matching revision-1 baseline/max-size/device routing
 * and a real memory provider before attempting virtio_msg_register().
 */
const struct virtio_msg_ops *vmsg_vsock_get_ops(void);
struct vmsg_vsock *vmsg_vsock_connect(struct virtio_msg_device *vmdev, u32 cid,
                                     u32 port, u16 maximum, unsigned long timeout);
/* Optional owner binding before the first transfer/discovery/registration. The virtio
 * device's parent must be this already enabled PCI device. The owner proves
 * matching host/device identity and a dedicated immutable BAR, and drains the
 * guest driver, mappings and peer before PCI removal. No search/enable/claim.
 * Local get_shm_region uses this binding exclusively; there is no peer fallback.
 * Raw nonempty GET_SHM replies still require the same matching binding.
 */
int vmsg_vsock_bind_shm(struct vmsg_vsock *, struct pci_dev *, u8 id, unsigned int bar);
/* Explicit software-parent variant, before any transfer/discovery/registration.
 * parent must be the exact published virtio parent and its direct parent must
 * be this PCI device. The trusted caller retains both immutable relationships,
 * parent storage/code and the original identity/BAR authority through destroy.
 * No ancestor search or parent reference is taken here: retirement can wait on
 * the parent's release before destroying the bus without creating a cycle.
 * This does not prove the software parent's DMA profile or Guest/GPU rundown.
 */
int vmsg_vsock_bind_shm_parent(struct vmsg_vsock *, struct pci_dev *,
                               struct device *parent, u8 id, unsigned int bar);
/* Call only after successful device registration and callback initialization. */
int vmsg_vsock_enable_events(struct vmsg_vsock *bus);
/* The owner serializes connect/enable/shutdown/destroy. Process context only;
 * never call shutdown/destroy from a bus worker/callback. Stops local I/O,
 * fails pending requests and joins workers. Does not prove remote quiescence,
 * revoke mappings, reset/unregister a device, or authorize freeing a virtqueue.
 */
void vmsg_vsock_shutdown(struct vmsg_vsock *bus);
/* No new/active transfer callers or device callbacks may remain at destruction. */
void vmsg_vsock_destroy(struct vmsg_vsock *bus);
#endif
