/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef WSX_VMSG_PCI_SHM_H
#define WSX_VMSG_PCI_SHM_H
#include <linux/pci.h>

/* One whole, dedicated 64-bit prefetchable BAR in the current HDV profile.
 * The PCI driver owns enablement, immutable assignment and removal/drain.
 * This reference is not a resource claim, ioremap, DMA mapping or PFN export.
 */
struct vmsg_pci_shm {
    struct pci_dev *device;
    u64 base, length;
    unsigned int bar;
    u8 id;
};
int vmsg_pci_shm_bind(struct vmsg_pci_shm *, struct pci_dev *, struct device *parent,
                      u8 id, unsigned int bar);
int vmsg_pci_shm_validate(const struct vmsg_pci_shm *, u32 id, u64 base, u64 length);
void vmsg_pci_shm_put(struct vmsg_pci_shm *);
#endif
