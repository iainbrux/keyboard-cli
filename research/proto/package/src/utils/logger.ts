// export const log = (type, info) => {
//   const now = new Date();

//   const year = now.getFullYear();
//   const month = String(now.getMonth() + 1).padStart(2, '0');
//   const day = String(now.getDate()).padStart(2, '0');
//   const hours = String(now.getHours()).padStart(2, '0');
//   const minutes = String(now.getMinutes()).padStart(2, '0');
//   const seconds = String(now.getSeconds()).padStart(2, '0');
//   const milliseconds = String(now.getMilliseconds()).padStart(3, '0');

//   const formattedTime = `[${year}-${month}-${day} ${hours}:${minutes}:${seconds} ${milliseconds}]`;

//   let logType = '';
//   let cssStyle = '';

//   switch (type) {
//     case 0:
//       logType = 'INFO';
//       cssStyle = 'color: blue;';
//       break;
//     case 1:
//       logType = 'SUCCESS';
//       cssStyle = 'color: green;';
//       break;
//     case 2:
//       logType = 'WARNING';
//       // cssStyle = 'color: orange;';
//       break;
//     case 3:
//       logType = 'ERROR';
//       cssStyle = 'color: red; font-weight: bold;';
//       break;
//     default:
//       logType = 'LOG';
//       cssStyle = '';
//       break;
//   }
// };
