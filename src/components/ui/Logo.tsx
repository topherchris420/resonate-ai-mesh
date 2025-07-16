import * as React from "react";
import { cn } from "@/lib/utils";

export const Logo = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 100 100"
      className={cn("h-8 w-8", className)}
      {...props}
    >
      <defs>
        <linearGradient id="logo-gradient" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" style={{ stopColor: 'hsl(var(--primary))' }} />
          <stop offset="50%" style={{ stopColor: 'hsl(var(--accent))' }} />
          <stop offset="100%" style={{ stopColor: 'hsl(var(--secondary))' }} />
        </linearGradient>
      </defs>
      <g stroke="url(#logo-gradient)" strokeWidth="5" fill="none" strokeLinecap="round">
        {/* Outer Circles */}
        <circle cx="50" cy="50" r="45" opacity="0.8" />
        <ellipse cx="50" cy="50" rx="45" ry="25"  opacity="0.6" />
        <ellipse cx="50" cy="50" rx="25" ry="45" opacity="0.6" />

        {/* Rotated Ellipses for mesh effect */}
        <ellipse cx="50" cy="50" rx="45" ry="25" transform="rotate(60 50 50)" opacity="0.5" />
        <ellipse cx="50" cy="50" rx="25" ry="45" transform="rotate(60 50 50)" opacity="0.5" />
        <ellipse cx="50" cy="50" rx="45" ry="25" transform="rotate(120 50 50)" opacity="0.5" />
        <ellipse cx="50" cy="50" rx="25" ry="45" transform="rotate(120 50 50)" opacity="0.5" />

         {/* Inner accent circle */}
         <circle cx="50" cy="50" r="15" strokeWidth="4" opacity="0.9" />
      </g>
    </svg>
  );
};
