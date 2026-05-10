import React from 'react';

interface Props {
  show: boolean;
  title: string;
  message: string;
}

export const Notification: React.FC<Props> = ({ show, title, message }) => {
  if (!show) return null;

  return (
    <div className="fixed top-5 right-5 bg-white rounded-xl shadow-lg p-4 flex items-center gap-3 animate-slideIn z-[2000]">
      <div className="w-10 h-10 bg-green-100 rounded-lg flex items-center justify-center text-green-600 text-xl">
        ✓
      </div>
      <div>
        <h4 className="font-semibold text-gray-800 text-sm">{title}</h4>
        <p className="text-sm text-gray-500">{message}</p>
      </div>

      <style>{`
        @keyframes slideIn {
          from { transform: translateX(120%); }
          to { transform: translateX(0); }
        }
        .animate-slideIn {
          animation: slideIn 0.3s ease-out;
        }
      `}</style>
    </div>
  );
};
