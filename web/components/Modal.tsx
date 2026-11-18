import type { ReactNode } from "react";
import { Dialog, DialogContent, DialogTitle } from "./ui/dialog";

interface ModalProps {
  onClose: () => void;
  children: ReactNode;
  width?: string;
}

export function Modal({ onClose, children, width }: ModalProps) {
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent style={width ? { width } : undefined}>
        <DialogTitle className="sr-only">dialog</DialogTitle>
        {children}
      </DialogContent>
    </Dialog>
  );
}
