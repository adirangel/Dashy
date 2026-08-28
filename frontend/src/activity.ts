export const activity = [0,1,0,2,0,0,1, 1,2,0,3,1,0,0, 0,1,2,2,0,1,0, 2,3,1,0,1,2,0, 0,2,4,3,1,0,1, 1,3,2,4,2,1,0, 2,4,3,2,4,2,1, 3,2,4,3,2,4,2, 4,3,4,2,3,4,3, 3,4,4,3,4,4,2, 4,4,3,4,4,3,4, 4,3,4,4,3,4,4];

export function currentStreak(values: number[]) {
  let streak = 0;
  for (let index = values.length - 1; index >= 0 && values[index] > 0; index -= 1) streak += 1;
  return streak;
}
